//! O-Dynamo companion bridge (named pipe `ODraft.Dynamo` on Windows).
//!
//! Commands talk JSON-lines to the companion host. Context (`context.levels`,
//! `context.project`) is pushed on connect; `companion.evaluate` returns lines,
//! circles and arcs. `companion.subscribe` keeps a background reader that stores
//! pushed `evaluation` messages into [`crate::Session::companion`] for preview.
//!
//! Commit and ghost overlay rendering land in later phases. Non-Windows builds
//! return a clear "not available" error (wasm stays green).

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{Value, json};

use super::project;
use super::*;
use crate::{Result, Session};

const DEFAULT_PIPE: &str = "ODraft.Dynamo";
const MAX_PIPE_NAME: usize = 64;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

/// Connection and preview state for the O-Dynamo companion.
#[derive(Clone)]
pub struct CompanionState {
    pub pipe_name: String,
    pub connected: bool,
    pub subscribed: bool,
    /// Last evaluate result or pushed evaluation (`result` object).
    pub preview: Option<Value>,
    slot: Arc<Mutex<Option<Value>>>,
    stop: Arc<AtomicBool>,
    next_id: Arc<AtomicU64>,
}

impl Default for CompanionState {
    fn default() -> Self {
        Self {
            pipe_name: DEFAULT_PIPE.into(),
            connected: false,
            subscribed: false,
            preview: None,
            slot: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(true)),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

impl CompanionState {
    fn pull_slot(&mut self) {
        let Ok(guard) = self.slot.lock() else {
            return;
        };
        if let Some(v) = guard.as_ref() {
            self.preview = Some(v.clone());
        }
    }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("companion.connect", "Connect Companion", run_connect)
            .menu(&["Architecture", "Companion", "Connect"])
            .params("{pipe?: string (default ODraft.Dynamo)} → {connected, pipe, levels, project}")
            .noundo(),
        CommandSpec::new("companion.disconnect", "Disconnect Companion", run_disconnect)
            .menu(&["Architecture", "Companion", "Disconnect"])
            .params("{} → {connected: false}")
            .noundo(),
        CommandSpec::new("companion.status", "Companion Status", run_status)
            .menu(&["Architecture", "Companion", "Status"])
            .params("{} → {connected, subscribed, pipe, preview}")
            .noundo(),
        CommandSpec::new("companion.evaluate", "Evaluate Companion Graph", run_evaluate)
            .menu(&["Architecture", "Companion", "Evaluate"])
            .params("{graph?: graph JSON (else the host's current graph)} → {ok, result}")
            .noundo(),
        CommandSpec::new("companion.subscribe", "Subscribe Companion Preview", run_subscribe)
            .menu(&["Architecture", "Companion", "Subscribe"])
            .params("{} → {subscribed}")
            .noundo(),
        CommandSpec::new("companion.unsubscribe", "Unsubscribe Companion Preview", run_unsubscribe)
            .menu(&["Architecture", "Companion", "Unsubscribe"])
            .params("{} → {subscribed: false}")
            .noundo(),
        CommandSpec::new("companion.preview", "Companion Preview", run_preview)
            .params("{} → {preview}")
            .noundo(),
    ]
}

fn run_connect(s: &mut Session, p: &Value) -> Result<Value> {
    let pipe = str_param(p, "pipe").unwrap_or(DEFAULT_PIPE).trim();
    if pipe.is_empty() || pipe.len() > MAX_PIPE_NAME || pipe.contains(['\\', '/', '\0']) {
        return Err(bad("companion.connect", "`pipe` must be a short pipe name without path separators"));
    }
    #[cfg(not(windows))]
    {
        let _ = (s, pipe);
        return Err(bad("companion.connect", "the companion pipe is only available on Windows"));
    }
    #[cfg(windows)]
    {
        stop_subscriber(&mut s.companion);
        s.companion.pipe_name = pipe.to_string();
        let id = next_id(&s.companion);
        let pong = pipe_roundtrip(&s.companion.pipe_name, &json!({ "v": 2, "id": id, "method": "ping" }))
            .map_err(|e| bad("companion.connect", e))?;
        if pong.get("ok") != Some(&Value::Bool(true)) {
            return Err(bad("companion.connect", format!("ping failed: {pong}")));
        }
        let levels = levels_payload(s)?;
        let project = project_payload(s)?;
        let id_l = next_id(&s.companion);
        let lvl_resp = pipe_roundtrip(
            &s.companion.pipe_name,
            &json!({ "v": 2, "id": id_l, "method": "context.levels", "params": { "levels": levels } }),
        )
        .map_err(|e| bad("companion.connect", e))?;
        if lvl_resp.get("ok") != Some(&Value::Bool(true)) {
            return Err(bad("companion.connect", format!("context.levels failed: {lvl_resp}")));
        }
        let id_p = next_id(&s.companion);
        let proj_resp = pipe_roundtrip(
            &s.companion.pipe_name,
            &json!({ "v": 2, "id": id_p, "method": "context.project", "params": { "project": project } }),
        )
        .map_err(|e| bad("companion.connect", e))?;
        if proj_resp.get("ok") != Some(&Value::Bool(true)) {
            return Err(bad("companion.connect", format!("context.project failed: {proj_resp}")));
        }
        s.companion.connected = true;
        Ok(json!({
            "connected": true,
            "pipe": s.companion.pipe_name,
            "levels": levels,
            "project": project,
        }))
    }
}

fn run_disconnect(s: &mut Session, _p: &Value) -> Result<Value> {
    stop_subscriber(&mut s.companion);
    s.companion.connected = false;
    s.companion.preview = None;
    Ok(json!({ "connected": false }))
}

fn run_status(s: &mut Session, _p: &Value) -> Result<Value> {
    s.companion.pull_slot();
    Ok(json!({
        "connected": s.companion.connected,
        "subscribed": s.companion.subscribed,
        "pipe": s.companion.pipe_name,
        "preview": s.companion.preview,
        "available": cfg!(windows),
    }))
}

fn run_evaluate(s: &mut Session, p: &Value) -> Result<Value> {
    #[cfg(not(windows))]
    {
        let _ = (s, p);
        return Err(bad("companion.evaluate", "the companion pipe is only available on Windows"));
    }
    #[cfg(windows)]
    {
        ensure_connected(s)?;
        let id = next_id(&s.companion);
        let mut req = json!({ "v": 2, "id": id, "method": "evaluate", "params": {} });
        if let Some(g) = p.get("graph") {
            req["params"]["graph"] = g.clone();
        }
        let resp = pipe_roundtrip(&s.companion.pipe_name, &req).map_err(|e| bad("companion.evaluate", e))?;
        if let Some(result) = resp.get("result").cloned() {
            s.companion.preview = Some(result.clone());
            if let Ok(mut slot) = s.companion.slot.lock() {
                *slot = Some(result);
            }
        }
        Ok(resp)
    }
}

fn run_subscribe(s: &mut Session, _p: &Value) -> Result<Value> {
    #[cfg(not(windows))]
    {
        let _ = s;
        return Err(bad("companion.subscribe", "the companion pipe is only available on Windows"));
    }
    #[cfg(windows)]
    {
        ensure_connected(s)?;
        if s.companion.subscribed {
            return Ok(json!({ "subscribed": true }));
        }
        let pipe = s.companion.pipe_name.clone();
        let slot = Arc::clone(&s.companion.slot);
        let stop = Arc::clone(&s.companion.stop);
        stop.store(false, Ordering::SeqCst);
        let id = next_id(&s.companion);
        std::thread::Builder::new()
            .name("companion-sub".into())
            .spawn(move || subscribe_loop(pipe, id, slot, stop))
            .map_err(|e| bad("companion.subscribe", format!("could not start subscriber: {e}")))?;
        // Give the thread a moment to open the pipe and send subscribe.
        std::thread::sleep(Duration::from_millis(100));
        s.companion.subscribed = true;
        Ok(json!({ "subscribed": true }))
    }
}

fn run_unsubscribe(s: &mut Session, _p: &Value) -> Result<Value> {
    stop_subscriber(&mut s.companion);
    Ok(json!({ "subscribed": false }))
}

fn run_preview(s: &mut Session, _p: &Value) -> Result<Value> {
    s.companion.pull_slot();
    Ok(json!({ "preview": s.companion.preview }))
}

fn ensure_connected(s: &Session) -> Result<()> {
    if s.companion.connected {
        Ok(())
    } else {
        Err(bad("companion", "not connected; run companion.connect first"))
    }
}

fn next_id(c: &CompanionState) -> u64 {
    c.next_id.fetch_add(1, Ordering::Relaxed)
}

fn stop_subscriber(c: &mut CompanionState) {
    c.stop.store(true, Ordering::SeqCst);
    c.subscribed = false;
}

fn levels_payload(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    let active = project::active_floor(d).map(|(_, n)| n);
    let mut elev = 0.0_f64;
    let mut out = Vec::new();
    for f in project::floors(d) {
        let name = f.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let height = f.get("floorToFloor").and_then(Value::as_f64).filter(|n| n.is_finite() && *n > 0.0).unwrap_or(3.0);
        let is_active = active.as_deref() == Some(name.as_str());
        out.push(json!({
            "name": name,
            "elevation": (elev * 1000.0).round() / 1000.0,
            "height": height,
            "active": is_active,
        }));
        elev += height;
    }
    Ok(Value::Array(out))
}

fn project_payload(s: &Session) -> Result<Value> {
    let d = s.doc()?;
    let details = project::get(d, "details");
    let structure = project::get(d, "structure");
    let floors = Value::Array(project::floors(d));
    let mut map = serde_json::Map::new();
    if let Value::Object(d) = details {
        for (k, v) in d {
            map.insert(k, v);
        }
    }
    map.insert("details".into(), project::get(d, "details"));
    map.insert("structure".into(), structure);
    map.insert("floors".into(), floors);
    Ok(Value::Object(map))
}

#[cfg(windows)]
fn pipe_roundtrip(pipe_name: &str, request: &Value) -> std::result::Result<Value, String> {
    use std::io::{BufRead, BufReader, Write};

    let path = format!(r"\\.\pipe\{pipe_name}");
    let payload = serde_json::to_string(request).map_err(|e| e.to_string())?;
    if payload.len() > MAX_RESPONSE_BYTES {
        return Err("request is too large".into());
    }
    let mut last = String::from("pipe not found");
    for _ in 0..40 {
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "{payload}").map_err(|e| e.to_string())?;
                let mut reader = BufReader::new(file);
                let mut line = String::new();
                reader.read_line(&mut line).map_err(|e| e.to_string())?;
                if line.len() > MAX_RESPONSE_BYTES {
                    return Err("response is too large".into());
                }
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return Err("empty response from companion".into());
                }
                return serde_json::from_str(trimmed).map_err(|e| format!("invalid companion JSON: {e}"));
            }
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(format!("cannot open pipe `{pipe_name}`: {last}"))
}

#[cfg(windows)]
fn subscribe_loop(pipe_name: String, subscribe_id: u64, slot: Arc<Mutex<Option<Value>>>, stop: Arc<AtomicBool>) {
    use std::io::{BufRead, BufReader, Write};

    let path = format!(r"\\.\pipe\{pipe_name}");
    let mut file = None;
    for _ in 0..40 {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
            Ok(f) => {
                file = Some(f);
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(50)),
        }
    }
    let Some(mut file) = file else {
        return;
    };
    let req = json!({ "v": 2, "id": subscribe_id, "method": "subscribe" });
    let Ok(payload) = serde_json::to_string(&req) else {
        return;
    };
    if writeln!(file, "{payload}").is_err() {
        return;
    }
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    while !stop.load(Ordering::SeqCst) {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if line.len() > MAX_RESPONSE_BYTES {
                    break;
                }
                let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
                    continue;
                };
                // First reply is the subscribe ack; later pushes are method=evaluation.
                if msg.get("method").and_then(Value::as_str) == Some("evaluation") {
                    let payload = msg.get("result").cloned().unwrap_or(msg);
                    if let Ok(mut guard) = slot.lock() {
                        *guard = Some(payload);
                    }
                }
            }
            Err(_) => break,
        }
    }
}
