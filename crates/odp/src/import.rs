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

/// The size of one annotated-PDF chunk record in the take-off record form (what O-Draft's recorder writes).
pub const PDF_CHUNK: usize = 512 * 1024;

impl Odp {
    /// The take-off in record form, `(id, payload)` with `id = kind << 56 | slot << 32 | n`, the shape O-Draft's recorder works in: one header per
    /// drawing slot, the rows, and each annotated PDF split into chunk records (kind 9).
    pub fn takeoff_as_records(&self) -> Result<Vec<(u64, Vec<u8>)>> {
        let mut out = Vec::new();
        for h in self.takeoff_headers()? {
            out.push((record_id(KIND_HEADER, h.slot, 0), h.json.to_string().into_bytes()));
            for r in self.takeoff_rows(h.slot)? {
                out.push((record_id(r.kind, r.slot, r.n), r.json.to_string().into_bytes()));
            }
        }
        for id in self.asset_ids()? {
            let Some(a) = self.asset(&id)? else { continue };
            let Some(slot) = a.slot.filter(|_| id.starts_with("annotated-")) else { continue };
            for (n, chunk) in a.bytes.chunks(PDF_CHUNK).enumerate() {
                out.push((record_id(KIND_PDF, slot, n as u32), chunk.to_vec()));
            }
        }
        Ok(out)
    }

    /// Make the take-off in the file equal `records` (the same shape [`Self::takeoff_as_records`] gives): changed headers and rows are written, ones
    /// that are gone are removed, and annotated PDFs are rebuilt from their chunks. One revision, and none when nothing differs.
    pub fn replace_takeoff(&mut self, records: &[(u64, Vec<u8>)]) -> Result<i64> {
        let mut headers: BTreeMap<u32, serde_json::Value> = BTreeMap::new();
        let mut rows: BTreeMap<(u32, u8, u32), serde_json::Value> = BTreeMap::new();
        let mut pdf: BTreeMap<u32, BTreeMap<u32, &[u8]>> = BTreeMap::new();
        for (id, payload) in records {
            let (kind, slot, n) = ((id >> 56) as u8, ((id >> 32) & 0x00FF_FFFF) as u32, (id & 0xFFFF_FFFF) as u32);
            match kind {
                KIND_HEADER => {
                    headers.insert(slot, serde_json::from_slice(payload).map_err(|e| Error::Import(format!("a take-off header is not JSON: {e}")))?);
                }
                k if ROW_KINDS.contains(&k) => {
                    rows.insert((slot, k, n), serde_json::from_slice(payload).map_err(|e| Error::Import(format!("a take-off row is not JSON: {e}")))?);
                }
                KIND_PDF => {
                    pdf.entry(slot).or_default().insert(n, payload.as_slice());
                }
                _ => {}
            }
        }
        let current_headers: BTreeMap<u32, TakeoffHeader> = self.takeoff_headers()?.into_iter().map(|h| (h.slot, h)).collect();
        let mut current_rows: BTreeMap<(u32, u8, u32), serde_json::Value> = BTreeMap::new();
        for slot in current_headers.keys() {
            for r in self.takeoff_rows(*slot)? {
                current_rows.insert((r.slot, r.kind, r.n), r.json);
            }
        }
        let mut edit = self.edit()?;
        for (slot, json) in &headers {
            if current_headers.get(slot).map(|h| &h.json) != Some(json) {
                let state = json.get("state").and_then(|s| s.as_str()).unwrap_or("complete").to_string();
                edit.put_takeoff_header(&TakeoffHeader { slot: *slot, state, json: json.clone() })?;
            }
        }
        for ((slot, kind, n), json) in &rows {
            if current_rows.get(&(*slot, *kind, *n)) != Some(json) {
                edit.put_takeoff_row(&TakeoffRow { slot: *slot, kind: *kind, n: *n, json: json.clone() })?;
            }
        }
        for key in current_rows.keys().filter(|k| !rows.contains_key(k)) {
            edit.delete_takeoff_row(key.0, key.1, key.2)?;
        }
        for slot in current_headers.keys().filter(|s| !headers.contains_key(s)) {
            edit.delete_takeoff_header(*slot)?;
        }
        for (slot, chunks) in &pdf {
            let bytes: Vec<u8> = chunks.values().flat_map(|c| c.iter().copied()).collect();
            let id = format!("annotated-{slot}.pdf");
            if edit.asset_hash(&id)?.as_deref() != Some(crate::sha256_hex(&bytes).as_str()) {
                edit.put_asset(&Asset { id: id.clone(), slot: Some(*slot), name: id, mime: "application/pdf".into(), sha256: String::new(), bytes })?;
            }
        }
        for id in edit.annotated_asset_ids()? {
            let slot = id.trim_start_matches("annotated-").trim_end_matches(".pdf").parse::<u32>().ok();
            if slot.is_some_and(|s| !pdf.contains_key(&s)) {
                edit.delete_asset(&id)?;
            }
        }
        edit.commit()
    }
}

fn record_id(kind: u8, slot: u32, n: u32) -> u64 {
    (u64::from(kind) << 56) | (u64::from(slot) << 32) | u64::from(n)
}
