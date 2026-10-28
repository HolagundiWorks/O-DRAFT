//! WinUI token / window-template detect check (same rules as `cargo xtask ui`).

use std::path::Path;

use serde_json::{Value, json};

/// Files that define the tokens and components; they are expected to contain numbers.
const TOKEN_SOURCES: &[&str] = &["Density.cs", "Standard.cs", "ShellFit.cs", "ToolIcons.cs", "ArchIcons.cs"];

/// Windows that may set themselves up without `StandardWindow.Present`, and why.
const WINDOW_EXCEPTIONS: &[&str] = &["App.xaml.cs", "MainWindow.xaml.cs", "CanvasMenu.cs", "CommandChrome.cs", "SmartBlockWindow.cs"];

/// Highest allowed count of hard-coded dimensions per file (ratchet; keep in sync with `xtask/src/ui.rs`).
/// Keep in sync with `xtask/src/ui.rs` LITERAL_BASELINE.
const LITERAL_BASELINE: &[(&str, usize)] = &[
    ("BatchPlotWindow.cs", 2),
    ("CanvasMenu.cs", 4),
    ("CommandChrome.cs", 2),
    ("DimStyleWindow.cs", 5),
    ("DraftingWindow.cs", 1),
    ("HatchWindow.cs", 2),
    ("LevelBars.cs", 2),
    ("LinetypeWindow.cs", 5),
    ("MLeaderStyleWindow.cs", 4),
    ("MainWindow.xaml.cs", 24),
    ("PageSetupWindow.cs", 5),
    ("ParametersWindow.cs", 5),
    ("PlotStyleWindow.cs", 7),
    ("PointStyleWindow.cs", 4),
    ("PomodoroWindow.cs", 1),
    ("QuickSelectWindow.cs", 6),
    ("QuickWindow.cs", 5),
    ("RoomSettingsWindow.cs", 4),
    ("SmartBlockWindow.cs", 4),
    ("TextStyleWindow.cs", 4),
    ("UnitsWindow.cs", 3),
];

const SIZE_PROPS: &[&str] = &[
    "Margin",
    "Padding",
    "Spacing",
    "RowSpacing",
    "ColumnSpacing",
    "Width",
    "Height",
    "MinWidth",
    "MinHeight",
    "MaxWidth",
    "MaxHeight",
    "FontSize",
    "CornerRadius",
];

const RAW_WINDOW_SETUP: &[&str] = &["ShellIcon.Apply(", "ShellFit.ToContent(", "ShellFit.UserResizable(", "ShellFit.Adaptive(", "ShellFit.Attach("];

fn starts_nonzero_number(rest: &str) -> bool {
    let rest = rest.trim_start();
    matches!(rest.as_bytes().first(), Some(b'1'..=b'9'))
}

fn is_ident(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn count_followed_by_number(line: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut from = 0;
    while let Some(pos) = line.get(from..).and_then(|s| s.find(needle)) {
        let at = from + pos;
        let boundary = at == 0 || line.as_bytes().get(at - 1).is_none_or(|b| !is_ident(*b));
        let after = at + needle.len();
        if boundary && line.get(after..).is_some_and(starts_nonzero_number) {
            count += 1;
        }
        from = after;
    }
    count
}

/// Hard-coded dimensions in one C# or XAML source.
pub fn count_literals(source: &str, xaml: bool) -> usize {
    let mut total = 0;
    for raw in source.lines() {
        let line = raw.trim_start();
        if line.starts_with("//") || line.starts_with("///") || line.starts_with("<!--") {
            continue;
        }
        if xaml {
            for prop in SIZE_PROPS {
                total += count_followed_by_number(line, &format!("{prop}=\""));
            }
        } else {
            for prop in SIZE_PROPS {
                total += count_followed_by_number(line, &format!("{prop} = "));
            }
            total += count_followed_by_number(line, "new Thickness(");
            total += count_followed_by_number(line, "new CornerRadius(");
        }
    }
    total
}

fn creates_window(source: &str) -> bool {
    source.lines().any(|raw| {
        let line = raw.trim();
        if line.starts_with("//") {
            return false;
        }
        let constructs = line.match_indices("new Window").any(|(at, needle)| {
            let next = line.as_bytes().get(at + needle.len()).copied();
            next.is_none_or(|b| !is_ident(b) && b != b'?')
        });
        constructs || line.ends_with(": Window")
    })
}

/// Problems with one file, given its name and contents.
pub fn check_file(name: &str, source: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let xaml = name.ends_with(".xaml");
    let exception = WINDOW_EXCEPTIONS.contains(&name);
    if !xaml && !TOKEN_SOURCES.contains(&name) && name != "ShellIcon.cs" && !exception {
        if creates_window(source) && !source.contains("StandardWindow.Present(") {
            problems.push(format!("{name}: creates a window without StandardWindow.Present (pick a WindowTemplate)"));
        }
        for call in RAW_WINDOW_SETUP {
            if source.contains(call) {
                problems.push(format!("{name}: calls {call}..) directly; use StandardWindow.Present"));
            }
        }
    }
    if !TOKEN_SOURCES.contains(&name) && !name.starts_with("Themes/") {
        let found = count_literals(source, xaml);
        let allowed = LITERAL_BASELINE.iter().find(|(file, _)| *file == name).map_or(0, |(_, n)| *n);
        if found > allowed {
            problems.push(format!("{name}: {found} hard-coded dimension(s), {allowed} allowed; use Density / Themes tokens or Ui components"));
        }
    }
    problems
}

/// Scan `apps/aad-winui` for design-system problems. Detect-only JSON; same rules as `cargo xtask ui`.
pub fn check_ui_tokens(root: &Path) -> Result<Value, String> {
    let dir = root.join("apps/aad-winui");
    let mut names = Vec::new();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.ends_with(".cs") || name.ends_with(".xaml") {
            names.push(name);
        }
    }
    names.sort();
    // Cap how many sources we open (hostile trees).
    const MAX_FILES: usize = 500;
    if names.len() > MAX_FILES {
        return Err(format!("apps/aad-winui has {} sources; auditor caps at {MAX_FILES}", names.len()));
    }
    let mut problems = Vec::new();
    for name in &names {
        let bytes = std::fs::read(dir.join(name)).map_err(|e| format!("{name}: {e}"))?;
        // Cap individual file size.
        if bytes.len() > 2_000_000 {
            problems.push(format!("{name}: skipped (file larger than 2 MiB)"));
            continue;
        }
        let source = String::from_utf8_lossy(&bytes);
        problems.extend(check_file(name, &source));
    }
    const MAX_PROBLEMS: usize = 200;
    let truncated = problems.len() > MAX_PROBLEMS;
    problems.truncate(MAX_PROBLEMS);
    let ok = problems.is_empty() && !truncated;
    Ok(json!({
        "tool": "check_ui_tokens",
        "detectOnly": true,
        "ok": ok,
        "filesScanned": names.len(),
        "problemCount": problems.len(),
        "problems": problems,
        "problemsTruncated": truncated,
        "message": if ok {
            format!("ui: {} WinUI sources follow the window templates and token rules", names.len())
        } else {
            format!("{} UI design-system problem(s) (docs/UI_STANDARDS.md); also run `cargo xtask ui`", problems.len())
        },
        "note": "Detect-only; same ratchet as cargo xtask ui. Does not modify sources."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_root_or_err;

    #[test]
    fn counts_numbers_but_not_tokens() {
        let cs = "var a = new StackPanel { Spacing = 6, Margin = Density.DialogMargin };\n\
                  var b = new Border { Padding = new Thickness(4, 0, 4, 0), BorderThickness = new Thickness(0) };\n\
                  // Width = 12 in a comment\n\
                  box.MinWidth = 80; box.FontSize = Density.FontBody; x.LineWidth = 3;";
        assert_eq!(count_literals(cs, false), 3);
    }

    #[test]
    fn new_window_needs_the_template() {
        let raw = "var window = new Window { Title = \"X\" };\nShellIcon.Apply(window, theme);";
        let problems = check_file("NewToolWindow.cs", raw);
        assert_eq!(problems.len(), 2, "{problems:?}");
    }

    #[test]
    fn scans_this_repo() {
        let root = repo_root_or_err().expect("root");
        let v = check_ui_tokens(&root).expect("ui");
        assert_eq!(v["tool"], "check_ui_tokens");
        assert_eq!(v["detectOnly"], true);
        assert!(v["filesScanned"].as_u64().unwrap_or(0) > 0);
    }
}
