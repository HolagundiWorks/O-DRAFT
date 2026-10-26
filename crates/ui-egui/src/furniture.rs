//! Bundled standard furniture DXF catalog (`assets/furniture/standard`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Deserialize;

/// One piece in the standard furniture set.
#[derive(Clone, Debug, Deserialize)]
pub struct FurnitureItem {
    pub id: String,
    pub title: String,
    pub file: String,
    pub category: String,
    #[serde(default)]
    pub size_mm: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct CatalogFile {
    name: String,
    #[serde(default)]
    attribution: String,
    items: Vec<FurnitureItem>,
}

/// Resolved catalog with absolute paths to DXF files that exist on disk.
#[derive(Clone, Debug)]
pub struct FurnitureCatalog {
    pub name: String,
    pub attribution: String,
    pub dir: PathBuf,
    pub items: Vec<(FurnitureItem, PathBuf)>,
}

static CATALOG: OnceLock<Option<FurnitureCatalog>> = OnceLock::new();

/// Load (once) the standard furniture catalog from beside the program or the source tree.
pub fn standard() -> Option<&'static FurnitureCatalog> {
    CATALOG.get_or_init(load).as_ref()
}

fn load() -> Option<FurnitureCatalog> {
    let dir = find_dir()?;
    let text = std::fs::read_to_string(dir.join("catalog.json")).ok()?;
    let raw: CatalogFile = serde_json::from_str(&text).ok()?;
    let items: Vec<(FurnitureItem, PathBuf)> = raw
        .items
        .into_iter()
        .filter_map(|item| {
            let path = dir.join(&item.file);
            path.is_file().then_some((item, path))
        })
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(FurnitureCatalog { name: raw.name, attribution: raw.attribution, dir, items })
}

/// Candidate folders for `catalog.json` (install layout first, then repo checkout).
fn find_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        candidates.push(parent.join("furniture").join("standard"));
        candidates.push(parent.join("assets").join("furniture").join("standard"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("furniture").join("standard"));
        candidates.push(cwd.join("assets").join("furniture").join("standard"));
    }
    // Dev builds: walk up from this crate to the repo `assets/furniture/standard`.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("../../assets/furniture/standard"));
    candidates.into_iter().find(|p| p.join("catalog.json").is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_catalog_loads_from_the_repo() {
        let cat = load().expect("repo furniture catalog");
        assert!(cat.items.len() >= 10, "expected a usable set, got {}", cat.items.len());
        assert!(cat.items.iter().any(|(i, _)| i.id == "sofa-3"));
        assert!(cat.items.iter().all(|(_, p)| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dxf"))));
    }
}
