//! Bringing the older files into a project file: a ShilpiDB `.vdb` (the take-off O-Draft records, with O-CoreOp's project record `0x60` and the
//! annotated-PDF chunks) and an O-CoreOp `.bbsproj` (the project JSON).

use std::collections::BTreeMap;
use std::path::Path;

use crate::{Asset, Error, Odp, Result, TakeoffHeader, TakeoffRow};

const KIND_HEADER: u8 = 1;
const KIND_PDF: u8 = 9;
const KIND_PROJECT: u8 = 0x60;
const ROW_KINDS: [u8; 8] = [3, 4, 5, 6, 7, 8, 10, 11];

/// What an import took.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    pub headers: usize,
    pub rows: usize,
    pub assets: usize,
    pub project: bool,
    /// Records of a kind this version does not know (history, a later version's kinds); they are not copied.
    pub skipped: usize,
}

impl Odp {
    /// Import a ShilpiDB `.vdb` as one revision. Take-off headers and rows keep their slot, kind and number; the annotated PDF chunks of a drawing
    /// are joined into one asset `annotated-<slot>.pdf`; O-CoreOp's project record becomes the project document.
    pub fn import_vdb(&mut self, path: &Path) -> Result<ImportReport> {
        let store = aad_vdb::load_path(path).map_err(|e| Error::Import(format!("{}: {e}", path.display())))?;
        let mut report = ImportReport::default();
        let mut pdf: BTreeMap<u32, BTreeMap<u32, Vec<u8>>> = BTreeMap::new();
        let mut project: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
        let mut edit = self.edit()?;
        for rec in store.records() {
            let kind = (rec.id >> 56) as u8;
            let slot = ((rec.id >> 32) & 0x00FF_FFFF) as u32;
            let n = (rec.id & 0xFFFF_FFFF) as u32;
            match kind {
                KIND_HEADER => {
                    let json: serde_json::Value = serde_json::from_slice(&rec.payload).map_err(|e| Error::Import(format!("a take-off header is not JSON: {e}")))?;
                    let state = json.get("state").and_then(|s| s.as_str()).unwrap_or("complete").to_string();
                    edit.put_takeoff_header(&TakeoffHeader { slot, state, json })?;
                    report.headers += 1;
                }
                k if ROW_KINDS.contains(&k) => {
                    let json: serde_json::Value = serde_json::from_slice(&rec.payload).map_err(|e| Error::Import(format!("a take-off row is not JSON: {e}")))?;
                    edit.put_takeoff_row(&TakeoffRow { slot, kind: k, n, json })?;
                    report.rows += 1;
                }
                KIND_PDF => {
                    pdf.entry(slot).or_default().insert(n, rec.payload.clone());
                }
                KIND_PROJECT => {
                    project.insert(n, rec.payload.clone());
                }
                _ => report.skipped += 1,
            }
        }
        for (slot, chunks) in pdf {
            let bytes: Vec<u8> = chunks.into_values().flatten().collect();
            edit.put_asset(&Asset {
                id: format!("annotated-{slot}.pdf"),
                slot: Some(slot),
                name: format!("annotated-{slot}.pdf"),
                mime: "application/pdf".into(),
                sha256: String::new(),
                bytes,
            })?;
            report.assets += 1;
        }
        if !project.is_empty() {
            let bytes: Vec<u8> = project.into_values().flatten().collect();
            let text = String::from_utf8(bytes).map_err(|_| Error::Import("the project record is not UTF-8".into()))?;
            edit.put_project_doc(&text)?;
            report.project = true;
        }
        edit.commit()?;
        Ok(report)
    }

    /// Import an O-CoreOp project file (`.bbsproj` / `.json`) as the project document.
    pub fn import_bbsproj(&mut self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)?;
        let mut edit = self.edit()?;
        edit.put_project_doc(&text)?;
        edit.commit()?;
        Ok(())
    }
}
