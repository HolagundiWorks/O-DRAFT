//! `aad-cli auditor` — Auditor v0 detect-only JSON report.
//!
//! Never repairs drawings. Screenshot capture is documented in the report; use a live Windows
//! shell (`aad --control PORT` + control `ui.screenshot` / MCP `screenshot`) separately.

use aad_auditor::{DetectChecks, repo_root_or_err, run_detect};
use aad_engine::Session;
use serde_json::json;

const AUDITOR_USAGE: &str = "usage: aad-cli auditor [--file FILE.dxf | --sample | --metric] [--checks LIST] [--out REPORT.json]
  Detect-only report: audit, validate, command/menu parity, WinUI tokens.
  --checks  comma list: audit,validate,parity,ui (default all)
  Exit 1 when blocking findings (drawing audit/validate or UI tokens); parity gaps are informational.
  Screenshot capture is not run here; see screenshotCapture in the JSON (Windows control/MCP).";

pub fn run(args: &[String]) -> Result<(), String> {
    let mut file: Option<&str> = None;
    let mut sample = false;
    let mut metric = false;
    let mut checks_raw = String::new();
    let mut out: Option<&str> = None;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        match a.as_str() {
            "--file" => {
                file = Some(args.get(i + 1).ok_or("--file needs a path")?.as_str());
                i += 2;
            }
            "--sample" => {
                sample = true;
                i += 1;
            }
            "--metric" => {
                metric = true;
                i += 1;
            }
            "--checks" => {
                checks_raw = args.get(i + 1).ok_or("--checks needs a list")?.clone();
                i += 2;
            }
            "--out" => {
                out = Some(args.get(i + 1).ok_or("--out needs a path")?.as_str());
                i += 2;
            }
            "-h" | "--help" => {
                println!("{AUDITOR_USAGE}");
                return Ok(());
            }
            other if other.starts_with('-') => return Err(format!("auditor: unknown option {other}\n{AUDITOR_USAGE}")),
            other => {
                if file.is_some() {
                    return Err(format!("auditor: unexpected argument `{other}`\n{AUDITOR_USAGE}"));
                }
                file = Some(other);
                i += 1;
            }
        }
    }
    if usize::from(file.is_some()) + usize::from(sample) + usize::from(metric) > 1 {
        return Err("auditor: use only one of --file, --sample, --metric".into());
    }
    let checks = DetectChecks::parse(&checks_raw)?;
    let mut s = Session::empty();
    if let Some(path) = file {
        s.execute("open", &json!({ "path": path })).map_err(|e| e.to_string())?;
    } else if metric {
        s.new_drawing(true);
    } else {
        // Default and --sample: bracket sample so drawing checks always have a document.
        let _ = sample;
        s.open_drawing(aad_engine::sample::default_sample(), "Bracket", None);
    }
    let root = if checks.parity || checks.ui_tokens { Some(repo_root_or_err()?) } else { None };
    let report = run_detect(&mut s, root.as_deref(), checks)?;
    let text = serde_json::to_string_pretty(&report.value).unwrap_or_else(|_| "{}".into());
    if let Some(path) = out {
        std::fs::write(path, &text).map_err(|e| format!("{path}: {e}"))?;
        eprintln!("wrote {path}");
    }
    println!("{text}");
    if report.ok() { Ok(()) } else { Err(report.value.get("message").and_then(|m| m.as_str()).unwrap_or("auditor findings").into()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_is_ok() {
        assert!(run(&["--help".into()]).is_ok());
    }

    #[test]
    fn drawing_only_report_succeeds() {
        // No repo checks → no dependency on cwd.
        assert!(run(&["--sample".into(), "--checks".into(), "audit,validate".into()]).is_ok());
    }
}
