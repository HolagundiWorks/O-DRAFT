//! Loopback JSON-lines control server: one request per line, one reply per line.
//! This is the transport the MCP server (`aad-cli mcp --connect`) wraps.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use aad_ui_egui::ControlRequest;
use serde_json::{Value, json};

/// Maximum JSON-line request size in bytes (including the trailing newline).
/// Kept in sync with `aad_host::control::LINE_CAP`.
pub const LINE_CAP: usize = 1_048_576;

pub fn start(port: u16, ctx: egui::Context) -> Receiver<ControlRequest> {
    let (tx, rx) = channel::<ControlRequest>();
    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            log::error!("control server failed to bind 127.0.0.1:{port}: {e}");
            return rx;
        }
    };
    log::info!("control server listening on 127.0.0.1:{port}");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || serve(stream, tx, ctx));
        }
    });
    rx
}

fn serve(stream: TcpStream, tx: Sender<ControlRequest>, ctx: egui::Context) {
    let Ok(read) = stream.try_clone() else { return };
    let mut out = stream;
    let mut reader = BufReader::new(read);
    loop {
        match read_capped_line(&mut reader) {
            LineRead::Eof | LineRead::IoErr => break,
            LineRead::TooLong => {
                let _ = writeln!(out, "{}", json!({"ok": false, "error": "request is too long"}));
            }
            LineRead::Ok(line) if line.trim().is_empty() => {}
            LineRead::Ok(line) => {
                let reply = match serde_json::from_str::<Value>(&line) {
                    Ok(msg) => {
                        let id = msg.get("id").cloned().unwrap_or(Value::Null);
                        let method = msg.get("method").and_then(Value::as_str).unwrap_or("").to_string();
                        let params = msg.get("params").cloned().unwrap_or(json!({}));
                        let (req, pending) = ControlRequest::new(method, params);
                        if tx.send(req).is_err() {
                            break;
                        }
                        ctx.request_repaint();
                        // On timeout the request is cancelled if the app hasn't started it, and the reply's
                        // `state` says whether it can still have run.
                        let mut r = pending.wait(Duration::from_secs(60));
                        if let Some(o) = r.as_object_mut() {
                            o.insert("id".into(), id);
                        }
                        r
                    }
                    Err(e) => json!({"ok": false, "error": format!("bad JSON: {e}")}),
                };
                if writeln!(out, "{reply}").is_err() {
                    break;
                }
            }
        }
    }
}

enum LineRead {
    Ok(String),
    TooLong,
    Eof,
    IoErr,
}

fn read_capped_line(reader: &mut impl BufRead) -> LineRead {
    let mut buf = Vec::new();
    let limit = (LINE_CAP as u64).saturating_add(1);
    let n = {
        let mut limited = reader.take(limit);
        match limited.read_until(b'\n', &mut buf) {
            Ok(n) => n,
            Err(_) => return LineRead::IoErr,
        }
    };
    if n == 0 {
        return LineRead::Eof;
    }
    if buf.len() > LINE_CAP {
        if buf.last() != Some(&b'\n') {
            let mut discard = Vec::new();
            let _ = reader.read_until(b'\n', &mut discard);
        }
        return LineRead::TooLong;
    }
    while matches!(buf.last(), Some(b'\n' | b'\r')) {
        buf.pop();
    }
    match String::from_utf8(buf) {
        Ok(s) => LineRead::Ok(s),
        Err(_) => LineRead::IoErr,
    }
}

#[cfg(test)]
mod tests {
    use super::{LINE_CAP, LineRead, read_capped_line};
    use std::io::{BufReader, Cursor};

    #[test]
    fn egui_control_line_cap_matches_host() {
        assert_eq!(LINE_CAP, 1_048_576);
    }

    #[test]
    fn read_capped_line_rejects_oversize() {
        let mut huge = vec![b'{'; LINE_CAP + 4];
        huge.push(b'\n');
        let mut r = BufReader::new(Cursor::new(huge));
        assert!(matches!(read_capped_line(&mut r), LineRead::TooLong));
    }

    #[test]
    fn read_capped_line_keeps_normal_json() {
        let mut r = BufReader::new(Cursor::new(b"{\"method\":\"cmdline.state\"}\n"));
        match read_capped_line(&mut r) {
            LineRead::Ok(s) => assert_eq!(s, "{\"method\":\"cmdline.state\"}"),
            LineRead::TooLong | LineRead::Eof | LineRead::IoErr => {
                panic!("expected a normal JSON line")
            }
        }
    }
}
