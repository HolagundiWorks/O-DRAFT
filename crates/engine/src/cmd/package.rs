//! ADCENTER (browse reusable content).

use serde_json::{Value, json};

use super::*;
use crate::{Result, Session};

const MAX_LIST: usize = 10_000;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec::new("adcenter", "Design Center", run_adcenter)
            .menu(&["Tools", "Design Center"])
            .alias(&["adc", "dc"])
            .params(
                "{path?: drawing to browse (default: this drawing), table?: blocks|layers|styles|dimstyles|all (default all)} → \
                 {path, blocks, layers, styles, dimstyles}",
            )
            .noundo(),
    ]
}

fn run_adcenter(s: &mut Session, p: &Value) -> Result<Value> {
    const CMD: &str = "adcenter";
    let table = str_param(p, "table").unwrap_or("all").to_ascii_lowercase();
    #[cfg(not(target_arch = "wasm32"))]
    let (path, doc) = if let Some(path) = str_param(p, "path") {
        let bytes = std::fs::read(path).map_err(|e| bad(CMD, format!("{path}: {e}")))?;
        if bytes.len() > 200_000_000 {
            return Err(bad(CMD, "file too large"));
        }
        let io = super::file::io().ok_or_else(|| bad(CMD, "file formats are not available in this build"))?;
        let d = (io.read)(&bytes, path).map_err(|e| bad(CMD, e.to_string()))?;
        (path.to_string(), d)
    } else {
        ("(current)".into(), s.doc()?.clone())
    };
    #[cfg(target_arch = "wasm32")]
    let (path, doc) = {
        if str_param(p, "path").is_some() {
            return Err(bad(CMD, "browsing another file is not available in this build"));
        }
        ("(current)".into(), s.doc()?.clone())
    };
    let want = |k: &str| table == "all" || table == k;
    let mut out = json!({ "path": path });
    if want("blocks") {
        let blocks: Vec<Value> = doc
            .blocks
            .values()
            .filter(|b| !b.anonymous)
            .take(MAX_LIST)
            .map(|b| json!({ "name": b.name, "entities": b.entities.len(), "xref": b.xref_path, "description": b.description }))
            .collect();
        out["blocks"] = Value::Array(blocks);
    }
    if want("layers") {
        let layers: Vec<Value> = doc
            .layers
            .iter()
            .take(MAX_LIST)
            .map(|l| json!({ "name": l.name, "on": l.on, "frozen": l.frozen, "locked": l.locked, "color": l.color.name() }))
            .collect();
        out["layers"] = Value::Array(layers);
    }
    if want("styles") {
        let styles: Vec<Value> =
            doc.text_styles.iter().take(MAX_LIST).map(|t| json!({ "name": t.name, "font": t.font, "height": t.height })).collect();
        out["styles"] = Value::Array(styles);
    }
    if want("dimstyles") {
        let styles: Vec<Value> = doc.dim_styles.iter().take(MAX_LIST).map(|t| json!({ "name": t.name })).collect();
        out["dimstyles"] = Value::Array(styles);
    }
    Ok(out)
}
