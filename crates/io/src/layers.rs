//! Which layer each display-list primitive was drawn from, for the layered exports (SVG groups,
//! PDF optional content).
//!
//! A primitive keeps the handle of the entity it came from; the layer is that entity's layer. The
//! contents of a layout viewport carry no entity handle and are grouped as `Viewport content`;
//! anything else without an entity is on layer `0`.

use std::collections::HashMap;

use aad_doc::Drawing;
use aad_render::DisplayList;

/// Most layer groups in one export; further layers share the last group (hostile layer tables).
pub(crate) const MAX_LAYER_GROUPS: usize = 1000;

/// The layer groups of a display list, in the order each layer first draws.
pub(crate) struct LayerIndex {
    /// Group names, as the layer table spells them.
    pub names: Vec<String>,
    /// For each primitive of the list, the index into `names`.
    pub of_prim: Vec<usize>,
}

pub(crate) fn index(d: &Drawing, list: &DisplayList) -> LayerIndex {
    let mut names: Vec<String> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut of_prim = Vec::with_capacity(list.prims.len());
    for p in &list.prims {
        let raw = if p.handle == aad_render::VIEWPORT_CONTENT {
            "Viewport content".to_string()
        } else {
            d.entity(p.handle).map(|e| e.common.layer.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "0".to_string())
        };
        let key = raw.to_ascii_lowercase();
        let i = match seen.get(&key) {
            Some(i) => *i,
            None if names.len() < MAX_LAYER_GROUPS => {
                let shown = d.layer(&raw).map_or(raw, |l| l.name.clone());
                names.push(shown);
                seen.insert(key, names.len() - 1);
                names.len() - 1
            }
            None => names.len().saturating_sub(1),
        };
        of_prim.push(i);
    }
    LayerIndex { names, of_prim }
}

/// An `id`-safe form of a layer name: ASCII letters, digits, `-` and `_`, never empty.
pub(crate) fn xml_id(name: &str, n: usize) -> String {
    let cleaned: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(60).collect();
    format!("layer{n}-{cleaned}")
}

/// `s` escaped for an XML attribute value.
pub(crate) fn xml_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().take(260) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c if c.is_control() => out.push(' '),
            c => out.push(c),
        }
    }
    out
}
