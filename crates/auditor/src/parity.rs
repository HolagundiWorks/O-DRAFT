//! Command / menu parity (same catalog as `cargo xtask parity`, detect-only JSON).

use std::path::Path;

use serde_json::{Value, json};

/// Leaf menu items with their path ("Draw > Circle > 3 Points").
pub fn menu_leaves(catalog: &str) -> Vec<String> {
    let lines: Vec<&str> = catalog.lines().filter(|l| !l.trim().is_empty()).collect();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        let ind = l.len() - l.trim_start().len();
        while stack.last().is_some_and(|(d, _)| *d >= ind) {
            stack.pop();
        }
        stack.push((ind, l.trim().to_string()));
        let next_ind = lines.get(i + 1).map(|n| n.len() - n.trim_start().len()).unwrap_or(0);
        if next_ind <= ind && ind > 0 {
            out.push(stack.iter().map(|(_, s)| s.as_str()).collect::<Vec<_>>().join(" > "));
        }
    }
    out
}

/// Compare `xtask/data/menu-catalog.txt` to the live command registry. Does not write
/// `docs/parity-checklist.md` (that remains `cargo xtask parity`).
pub fn check_command_parity(root: &Path) -> Result<Value, String> {
    let catalog_path = root.join("xtask/data/menu-catalog.txt");
    let catalog = std::fs::read_to_string(&catalog_path).map_err(|e| format!("{}: {e}", catalog_path.display()))?;
    let specs = aad_engine::command_specs();
    let paths: Vec<String> = specs.iter().filter(|c| !c.menu.is_empty()).map(|c| c.menu.to_vec().join(" > ").to_ascii_lowercase()).collect();
    let labels: Vec<String> = paths.iter().filter_map(|p| p.rsplit(" > ").next().map(str::to_string)).collect();
    let ls = menu_leaves(&catalog);
    let mut missing = Vec::new();
    let mut hit = 0usize;
    for l in &ls {
        let lower = l.to_ascii_lowercase();
        let leaf = lower.rsplit(" > ").next().unwrap_or("").to_string();
        let ok = paths.contains(&lower) || labels.contains(&leaf);
        if ok {
            hit += 1;
        } else {
            missing.push(l.clone());
        }
    }
    let total = ls.len();
    let pct = if total == 0 { 0.0 } else { 100.0 * hit as f64 / total as f64 };
    // Cap missing list so hostile/huge catalogs cannot explode agent/MCP payloads.
    const MAX_MISSING: usize = 200;
    let truncated = missing.len() > MAX_MISSING;
    missing.truncate(MAX_MISSING);
    Ok(json!({
        "tool": "check_command_parity",
        "detectOnly": true,
        "ok": missing.is_empty() && !truncated,
        "covered": hit,
        "total": total,
        "percent": (pct * 10.0).round() / 10.0,
        "registeredCommands": specs.len(),
        "missing": missing,
        "missingTruncated": truncated,
        "message": format!("Menu breadth: {hit} / {total} items ({pct:.0}%). Regenerate docs with `cargo xtask parity`."),
        "note": "Detect-only; does not rewrite docs/parity-checklist.md."
    }))
}

/// Alias used by the agent registry (same backing check).
pub fn check_command_menu_coverage(root: &Path) -> Result<Value, String> {
    let mut v = check_command_parity(root)?;
    if let Some(o) = v.as_object_mut() {
        o.insert("tool".into(), json!("check_command_menu_coverage"));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo_root_or_err;

    #[test]
    fn leaves_with_paths() {
        let c = "Draw\n    Line\n    Circle\n        3 Points\n        2 Points\nModify\n    Move\n";
        assert_eq!(menu_leaves(c), vec!["Draw > Line", "Draw > Circle > 3 Points", "Draw > Circle > 2 Points", "Modify > Move"]);
    }

    #[test]
    fn parity_runs_against_this_repo() {
        let root = repo_root_or_err().expect("root");
        let v = check_command_parity(&root).expect("parity");
        assert_eq!(v["tool"], "check_command_parity");
        assert_eq!(v["detectOnly"], true);
        assert!(v["total"].as_u64().unwrap_or(0) > 0);
        assert!(v["registeredCommands"].as_u64().unwrap_or(0) > 0);
        let cover = check_command_menu_coverage(&root).expect("coverage");
        assert_eq!(cover["tool"], "check_command_menu_coverage");
        assert_eq!(cover["covered"], v["covered"]);
    }
}
