//! Auditor v0 — detect-only findings for drawings, command/menu parity and WinUI tokens.
//!
//! The auditor **never** repairs geometry or mutates a drawing. Drawing checks call engine
//! `audit` / `validate` with `fix: false`. Repo checks report parity and UI-token problems.
//! Screenshot capture stays on the live control channel (`ui.screenshot` / MCP `screenshot`);
//! this crate only documents that hook in report metadata.
//!
//! See `docs/ai-agent.md`, `docs/mcp.md` and `docs/TESTING.md`.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

mod drawing;
mod parity;
mod report;
mod root;
mod ui_tokens;

pub use drawing::{audit_drawing, validate_drawing};
pub use parity::{check_command_menu_coverage, check_command_parity};
pub use report::{DetectChecks, Report, run_detect};
pub use root::{find_repo_root, repo_root_or_err};
pub use ui_tokens::check_ui_tokens;

/// Tool / check names exposed by Auditor v0 (agent, MCP and `aad-cli auditor`).
pub const DETECT_TOOLS: &[&str] =
    &["audit_drawing", "validate_drawing", "check_command_parity", "check_command_menu_coverage", "check_ui_tokens", "auditor_report"];
