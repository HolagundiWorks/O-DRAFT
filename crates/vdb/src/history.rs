//! Drawing version history in a sidecar `.history.vdb` (ShilpiDB).
//!
//! Opaque payloads only: the engine supplies entity / definition / layout JSON bytes.
//! Ids use kinds `0x40`–`0x45` so they never clash with take-off kinds 1–9.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use shilpidb::Bbox;

use crate::file::{self, VdbRecord, VdbStore};
use crate::{MAX_PAYLOAD, MAX_RECORDS};

/// History header (one per file).
pub const KIND_HEADER: u8 = 0x40;
/// Milestone manifest.
pub const KIND_MANIFEST: u8 = 0x41;
/// Member-list chunk (object-row ids in draw order).
pub const KIND_MEMBERS: u8 = 0x42;
/// Shared object row (entity JSON).
pub const KIND_OBJECT: u8 = 0x43;
/// Shared definition row (layer, style, block, …).
pub const KIND_DEF: u8 = 0x44;
/// Layout row (project-scope milestones).
pub const KIND_LAYOUT: u8 = 0x45;

pub const FORMAT: &str = "odraft-history";
pub const FORMAT_VERSION: u32 = 1;

pub const MAX_MILESTONES: usize = 10_000;
pub const MAX_STORES: usize = 4_096;
/// Object-row ids per member chunk (8 bytes each; under [`MAX_PAYLOAD`]).
pub const MEMBER_CHUNK: usize = 65_536;

const ZERO_BOX: [f64; 4] = [0.0, 0.0, 0.0, 0.0];

/// Errors from reading or writing a history database.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Vdb(#[from] file::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl Error {
    fn msg(s: impl Into<String>) -> Self {
        Error::Msg(s.into())
    }
}

/// `id = (kind << 56) | n` with 56 bits for `n`.
pub fn record_id(kind: u8, n: u64) -> u64 {
    (u64::from(kind) << 56) | (n & 0x00FF_FFFF_FFFF_FFFF)
}

pub fn kind_of(id: u64) -> u8 {
    (id >> 56) as u8
}

pub fn n_of(id: u64) -> u64 {
    id & 0x00FF_FFFF_FFFF_FFFF
}

/// Member-chunk `n`: milestone (32) | store index (12) | chunk (12).
pub fn member_n(milestone: u32, store: u16, chunk: u16) -> u64 {
    (u64::from(milestone) << 24) | (u64::from(store & 0x0FFF) << 12) | u64::from(chunk & 0x0FFF)
}

/// FNV-1a 64-bit, then cut to 56 bits for object/definition row ids.
pub fn content_n(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h & 0x00FF_FFFF_FFFF_FFFF
}

fn bbox_from(corners: [f64; 4]) -> Result<Bbox, Error> {
    let [a, b, c, d] = corners;
    if ![a, b, c, d].iter().all(|v| v.is_finite()) {
        return Err(Error::msg("non-finite bbox"));
    }
    Ok(Bbox::from_corners(a, b, c, d))
}

fn union_box(boxes: &[[f64; 4]]) -> [f64; 4] {
    let mut out = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut any = false;
    for b in boxes {
        if !b.iter().all(|v| v.is_finite()) {
            continue;
        }
        if b[0] > b[2] || b[1] > b[3] {
            continue;
        }
        any = true;
        out[0] = out[0].min(b[0]);
        out[1] = out[1].min(b[1]);
        out[2] = out[2].max(b[2]);
        out[3] = out[3].max(b[3]);
    }
    if any { out } else { ZERO_BOX }
}

/// One object (or definition / layout) ready to share across milestones.
#[derive(Clone, Debug)]
pub struct RowIn {
    pub bbox: [f64; 4],
    pub payload: Vec<u8>,
}

/// One store inside a milestone (level design or sheet-set annotation).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoreMeta {
    /// Stable target key, e.g. `level:3` or `sheet:2:3`.
    pub target: String,
    pub name: String,
    pub object_count: u32,
    pub content_hash: String,
    pub member_chunks: u32,
    pub definitions: u32,
}

/// What the engine passes when saving a milestone.
#[derive(Clone, Debug)]
pub struct MilestoneIn {
    pub name: String,
    /// `auto`, `milestone`, or `revision`.
    pub kind: String,
    pub time: String,
    pub author: String,
    pub scope: Value,
    pub parent: Option<u32>,
    pub stores: Vec<StoreBundle>,
    /// Project-scope layout rows (optional).
    pub layouts: Vec<RowIn>,
}

#[derive(Clone, Debug)]
pub struct StoreBundle {
    pub meta: StoreMeta,
    pub objects: Vec<RowIn>,
    pub definitions: Vec<RowIn>,
}

/// Summary listed in the header and returned by [`HistoryDb::list`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MilestoneSummary {
    pub id: u32,
    pub name: String,
    pub kind: String,
    pub time: String,
    pub author: String,
    pub scope: Value,
    pub parent: Option<u32>,
    pub stores: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Header {
    format: String,
    version: u32,
    drawing_id: String,
    state: String,
    next_milestone: u32,
    milestones: Vec<MilestoneSummary>,
}

/// Fully loaded milestone for restore / compare.
#[derive(Clone, Debug)]
pub struct MilestoneOut {
    pub summary: MilestoneSummary,
    pub stores: Vec<StoreOut>,
    pub layouts: Vec<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct StoreOut {
    pub meta: StoreMeta,
    /// Object payloads in draw order.
    pub objects: Vec<Vec<u8>>,
    pub definitions: Vec<Vec<u8>>,
}

/// Open history file beside a drawing.
pub struct HistoryDb {
    path: PathBuf,
    drawing_id: String,
    records: BTreeMap<u64, VdbRecord>,
    header: Header,
}

impl HistoryDb {
    /// Open or create `<path>`. Refuses a non-empty file that is not this format or another drawing.
    pub fn open(path: &Path, drawing_id: &str) -> Result<Self, Error> {
        if drawing_id.is_empty() || drawing_id.len() > 128 {
            return Err(Error::msg("drawing_id must be 1..128 characters"));
        }
        let mut records = BTreeMap::new();
        let header = if path.exists() {
            let store = file::load_path(path)?;
            if store.len() > MAX_RECORDS {
                return Err(Error::msg(format!("history file has too many records (limit {MAX_RECORDS})")));
            }
            for r in store.records() {
                records.insert(r.id, r.clone());
            }
            if records.is_empty() {
                empty_header(drawing_id)
            } else {
                let Some(hr) = records.get(&record_id(KIND_HEADER, 0)) else {
                    return Err(Error::msg(format!("{} is not a history database (missing header)", path.display())));
                };
                let h: Header = serde_json::from_slice(&hr.payload)?;
                if h.format != FORMAT {
                    return Err(Error::msg(format!("{} is not a history database (format {:?})", path.display(), h.format)));
                }
                if h.drawing_id != drawing_id {
                    return Err(Error::msg(format!(
                        "{} belongs to another drawing (expected {drawing_id}, found {})",
                        path.display(),
                        h.drawing_id
                    )));
                }
                if h.state != "complete" && h.state != "writing" {
                    return Err(Error::msg(format!("{} has an unknown header state", path.display())));
                }
                // Incomplete write: keep rows; next save GC will drop orphans. Treat as complete for reading.
                let mut h = h;
                h.state = "complete".into();
                h
            }
        } else {
            empty_header(drawing_id)
        };
        Ok(Self { path: path.to_path_buf(), drawing_id: drawing_id.to_string(), records, header })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn drawing_id(&self) -> &str {
        &self.drawing_id
    }

    pub fn list(&self) -> &[MilestoneSummary] {
        &self.header.milestones
    }

    /// Append a milestone. Failure leaves the previous on-disk file untouched (in-memory may be dirty until save).
    pub fn save_milestone(&mut self, input: MilestoneIn) -> Result<u32, Error> {
        if self.header.milestones.len() >= MAX_MILESTONES {
            return Err(Error::msg(format!("at most {MAX_MILESTONES} milestones per history file")));
        }
        if input.stores.len() > MAX_STORES {
            return Err(Error::msg(format!("at most {MAX_STORES} stores per milestone")));
        }
        if input.name.is_empty() || input.name.len() > 200 {
            return Err(Error::msg("milestone name must be 1..200 characters"));
        }
        let kind = match input.kind.as_str() {
            "auto" | "milestone" | "revision" => input.kind.clone(),
            _ => return Err(Error::msg("kind must be auto, milestone or revision")),
        };
        let id = self.header.next_milestone;
        if id == u32::MAX {
            return Err(Error::msg("milestone numbers exhausted"));
        }

        self.set_state("writing")?;

        let mut store_boxes = Vec::new();
        let mut stores_json: Vec<Value> = Vec::with_capacity(input.stores.len());

        for (si, store) in input.stores.iter().enumerate() {
            if si > 0x0FFF {
                return Err(Error::msg("too many stores"));
            }
            let store_i = si as u16;
            let mut member_ids: Vec<u64> = Vec::with_capacity(store.objects.len());
            let mut obj_boxes = Vec::with_capacity(store.objects.len());

            for obj in &store.objects {
                let rid = self.put_shared(KIND_OBJECT, obj)?;
                member_ids.push(rid);
                obj_boxes.push(obj.bbox);
            }
            let mut def_ids: Vec<u64> = Vec::with_capacity(store.definitions.len());
            for def in &store.definitions {
                def_ids.push(self.put_shared(KIND_DEF, def)?);
            }

            let chunks = write_member_chunks(&mut self.records, id, store_i, &member_ids)?;
            let union = union_box(&obj_boxes);
            store_boxes.push(union);

            let mut meta = store.meta.clone();
            meta.object_count = u32::try_from(store.objects.len()).map_err(|_| Error::msg("too many objects in a store"))?;
            meta.member_chunks = chunks;
            meta.definitions = u32::try_from(def_ids.len()).map_err(|_| Error::msg("too many definitions"))?;
            let mut entry = serde_json::to_value(&meta)?;
            if let Some(o) = entry.as_object_mut() {
                o.insert("definition_ids".into(), json!(def_ids));
            }
            stores_json.push(entry);
        }

        // Layout rows (project scope).
        let mut layout_ids = Vec::new();
        for lay in &input.layouts {
            layout_ids.push(self.put_shared(KIND_LAYOUT, lay)?);
        }

        let manifest = json!({
            "name": input.name,
            "kind": kind,
            "time": input.time,
            "author": input.author,
            "scope": input.scope,
            "parent": input.parent,
            "stores": stores_json,
            "layouts": layout_ids,
        });
        let payload = serde_json::to_vec(&manifest)?;
        if payload.len() > MAX_PAYLOAD {
            return Err(Error::msg("manifest payload too large"));
        }
        let mbox = union_box(&store_boxes);
        self.records.insert(
            record_id(KIND_MANIFEST, u64::from(id)),
            VdbRecord { id: record_id(KIND_MANIFEST, u64::from(id)), bbox: bbox_from(mbox)?, payload },
        );

        let summary = MilestoneSummary {
            id,
            name: input.name,
            kind,
            time: input.time,
            author: input.author,
            scope: input.scope,
            parent: input.parent,
            stores: input.stores.len(),
        };
        self.header.milestones.push(summary);
        self.header.next_milestone = id.saturating_add(1);
        self.set_state("complete")?;
        self.flush()?;
        Ok(id)
    }

    /// Load a milestone's object and definition payloads.
    pub fn load_milestone(&self, id: u32) -> Result<MilestoneOut, Error> {
        let summary = self
            .header
            .milestones
            .iter()
            .find(|m| m.id == id)
            .cloned()
            .ok_or_else(|| Error::msg(format!("milestone {id} not found")))?;
        let hr = self
            .records
            .get(&record_id(KIND_MANIFEST, u64::from(id)))
            .ok_or_else(|| Error::msg(format!("milestone {id}: missing manifest")))?;
        let man: Value = serde_json::from_slice(&hr.payload)?;
        let stores_json = man.get("stores").and_then(Value::as_array).ok_or_else(|| Error::msg("manifest stores missing"))?;
        if stores_json.len() > MAX_STORES {
            return Err(Error::msg("manifest has too many stores"));
        }
        let mut stores = Vec::with_capacity(stores_json.len());
        for (si, s) in stores_json.iter().enumerate() {
            let meta: StoreMeta = serde_json::from_value(s.clone())?;
            let members = read_member_chunks(&self.records, id, si as u16, meta.member_chunks)?;
            let mut objects = Vec::with_capacity(members.len());
            for rid in members {
                let rec = self.records.get(&rid).ok_or_else(|| Error::msg(format!("milestone {id}: missing object row {rid:#x}")))?;
                if kind_of(rid) != KIND_OBJECT {
                    return Err(Error::msg("member list points at a non-object row"));
                }
                objects.push(rec.payload.clone());
            }
            let definitions = match s.get("definition_ids").and_then(Value::as_array) {
                Some(ids) => {
                    let mut defs = Vec::new();
                    for v in ids {
                        let rid = v.as_u64().ok_or_else(|| Error::msg("bad definition id"))?;
                        let rec = self.records.get(&rid).ok_or_else(|| Error::msg("missing definition row"))?;
                        defs.push(rec.payload.clone());
                    }
                    defs
                }
                None => Vec::new(),
            };
            stores.push(StoreOut { meta, objects, definitions });
        }

        let mut layouts = Vec::new();
        if let Some(arr) = man.get("layouts").and_then(Value::as_array) {
            for v in arr {
                let rid = v.as_u64().ok_or_else(|| Error::msg("bad layout id"))?;
                let rec = self.records.get(&rid).ok_or_else(|| Error::msg("missing layout row"))?;
                layouts.push(rec.payload.clone());
            }
        }
        Ok(MilestoneOut { summary, stores, layouts })
    }

    /// Rename a restore point. Auto points become named milestones. Refuses `revision` kind.
    pub fn rename_milestone(&mut self, id: u32, name: &str) -> Result<MilestoneSummary, Error> {
        if name.is_empty() || name.len() > 200 {
            return Err(Error::msg("milestone name must be 1..200 characters"));
        }
        let idx = self
            .header
            .milestones
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| Error::msg(format!("milestone {id} not found")))?;
        if self.header.milestones[idx].kind == "revision" {
            return Err(Error::msg("issued revisions cannot be renamed; void them in the register instead"));
        }
        self.set_state("writing")?;
        let rid = record_id(KIND_MANIFEST, u64::from(id));
        let Some(hr) = self.records.get(&rid).cloned() else {
            return Err(Error::msg(format!("milestone {id}: missing manifest")));
        };
        let mut man: Value = serde_json::from_slice(&hr.payload)?;
        if let Some(o) = man.as_object_mut() {
            o.insert("name".into(), json!(name));
            o.insert("kind".into(), json!("milestone"));
        }
        let payload = serde_json::to_vec(&man)?;
        if payload.len() > MAX_PAYLOAD {
            return Err(Error::msg("manifest payload too large"));
        }
        self.records.insert(rid, VdbRecord { id: rid, bbox: hr.bbox, payload });
        self.header.milestones[idx].name = name.to_string();
        self.header.milestones[idx].kind = "milestone".into();
        self.set_state("complete")?;
        self.flush()?;
        Ok(self.header.milestones[idx].clone())
    }

    /// Delete a named or auto milestone. Refuses `revision` kind.
    pub fn delete_milestone(&mut self, id: u32) -> Result<(), Error> {
        let idx = self.header.milestones.iter().position(|m| m.id == id).ok_or_else(|| Error::msg(format!("milestone {id} not found")))?;
        if self.header.milestones[idx].kind == "revision" {
            return Err(Error::msg("issued revisions cannot be deleted; void them in the register instead"));
        }
        self.set_state("writing")?;
        // Drop manifest and member chunks; GC removes unreferenced object/def rows.
        if let Some(hr) = self.records.get(&record_id(KIND_MANIFEST, u64::from(id)))
            && let Ok(man) = serde_json::from_slice::<Value>(&hr.payload)
            && let Some(stores) = man.get("stores").and_then(Value::as_array)
        {
            for (si, s) in stores.iter().enumerate() {
                let chunks = s.get("member_chunks").and_then(Value::as_u64).unwrap_or(0) as u32;
                for c in 0..chunks {
                    self.records.remove(&record_id(KIND_MEMBERS, member_n(id, si as u16, c as u16)));
                }
            }
        }
        self.records.remove(&record_id(KIND_MANIFEST, u64::from(id)));
        self.header.milestones.remove(idx);
        self.gc();
        self.set_state("complete")?;
        self.flush()?;
        Ok(())
    }

    /// Drop old automatic restore points: keep the newest `keep_recent`, plus one per UTC day for `keep_daily_days`.
    /// Named milestones and revisions are never removed.
    pub fn prune_auto(&mut self, keep_recent: usize, keep_daily_days: u32) -> Result<usize, Error> {
        let keep_recent = keep_recent.min(MAX_MILESTONES);
        let keep_daily_days = keep_daily_days.min(3650);
        let mut autos: Vec<&MilestoneSummary> = self.header.milestones.iter().filter(|m| m.kind == "auto").collect();
        autos.sort_by_key(|a| std::cmp::Reverse(a.id));
        let mut keep: BTreeSet<u32> = BTreeSet::new();
        for m in autos.iter().take(keep_recent) {
            keep.insert(m.id);
        }
        // One per calendar day (prefix of ISO time `YYYY-MM-DD`).
        let mut days: BTreeMap<String, u32> = BTreeMap::new();
        for m in &autos {
            let day = m.time.chars().take(10).collect::<String>();
            if day.len() == 10 {
                days.entry(day).or_insert(m.id);
            }
        }
        let mut day_ids: Vec<(String, u32)> = days.into_iter().collect();
        day_ids.sort_by(|a, b| b.0.cmp(&a.0));
        for (_, id) in day_ids.into_iter().take(keep_daily_days as usize) {
            keep.insert(id);
        }
        let drop: Vec<u32> = autos.iter().map(|m| m.id).filter(|id| !keep.contains(id)).collect();
        let n = drop.len();
        for id in drop {
            // Inline delete without per-id flush.
            let idx = match self.header.milestones.iter().position(|m| m.id == id) {
                Some(i) => i,
                None => continue,
            };
            if let Some(hr) = self.records.get(&record_id(KIND_MANIFEST, u64::from(id)))
                && let Ok(man) = serde_json::from_slice::<Value>(&hr.payload)
                && let Some(stores) = man.get("stores").and_then(Value::as_array)
            {
                for (si, s) in stores.iter().enumerate() {
                    let chunks = s.get("member_chunks").and_then(Value::as_u64).unwrap_or(0) as u32;
                    for c in 0..chunks {
                        self.records.remove(&record_id(KIND_MEMBERS, member_n(id, si as u16, c as u16)));
                    }
                }
            }
            self.records.remove(&record_id(KIND_MANIFEST, u64::from(id)));
            self.header.milestones.remove(idx);
        }
        if n > 0 {
            self.gc();
            self.set_state("complete")?;
            self.flush()?;
        }
        Ok(n)
    }

    fn put_shared(&mut self, kind: u8, row: &RowIn) -> Result<u64, Error> {
        if row.payload.len() > MAX_PAYLOAD {
            return Err(Error::msg(format!("payload {} bytes exceeds limit {MAX_PAYLOAD}", row.payload.len())));
        }
        if self.records.len() >= MAX_RECORDS {
            return Err(Error::msg(format!("history file would exceed {MAX_RECORDS} records")));
        }
        let mut n = content_n(&row.payload);
        // Probe on collision with different bytes.
        for _ in 0..1024 {
            let id = record_id(kind, n);
            match self.records.get(&id) {
                None => {
                    self.records.insert(id, VdbRecord { id, bbox: bbox_from(row.bbox)?, payload: row.payload.clone() });
                    return Ok(id);
                }
                Some(existing) if existing.payload == row.payload => return Ok(id),
                Some(_) => {
                    n = n.wrapping_add(1) & 0x00FF_FFFF_FFFF_FFFF;
                }
            }
        }
        Err(Error::msg("could not place a shared row (hash collisions)"))
    }

    fn set_state(&mut self, state: &str) -> Result<(), Error> {
        self.header.state = state.into();
        let payload = serde_json::to_vec(&self.header)?;
        if payload.len() > MAX_PAYLOAD {
            return Err(Error::msg("history header too large"));
        }
        let id = record_id(KIND_HEADER, 0);
        self.records.insert(id, VdbRecord { id, bbox: bbox_from(ZERO_BOX)?, payload });
        Ok(())
    }

    /// Remove object/def/layout rows not referenced by any remaining manifest.
    fn gc(&mut self) {
        let mut live: BTreeSet<u64> = BTreeSet::new();
        live.insert(record_id(KIND_HEADER, 0));
        for m in &self.header.milestones {
            live.insert(record_id(KIND_MANIFEST, u64::from(m.id)));
            if let Some(hr) = self.records.get(&record_id(KIND_MANIFEST, u64::from(m.id)))
                && let Ok(man) = serde_json::from_slice::<Value>(&hr.payload)
            {
                if let Some(stores) = man.get("stores").and_then(Value::as_array) {
                    for (si, s) in stores.iter().enumerate() {
                        let chunks = s.get("member_chunks").and_then(Value::as_u64).unwrap_or(0) as u32;
                        for c in 0..chunks {
                            let mid = record_id(KIND_MEMBERS, member_n(m.id, si as u16, c as u16));
                            live.insert(mid);
                            if let Some(chunk) = self.records.get(&mid) {
                                for id in decode_member_ids(&chunk.payload) {
                                    live.insert(id);
                                }
                            }
                        }
                        if let Some(defs) = s.get("definition_ids").and_then(Value::as_array) {
                            for v in defs {
                                if let Some(id) = v.as_u64() {
                                    live.insert(id);
                                }
                            }
                        }
                    }
                }
                if let Some(layouts) = man.get("layouts").and_then(Value::as_array) {
                    for v in layouts {
                        if let Some(id) = v.as_u64() {
                            live.insert(id);
                        }
                    }
                }
            }
        }
        self.records.retain(|id, _| live.contains(id));
    }

    fn flush(&self) -> Result<(), Error> {
        if self.records.len() > MAX_RECORDS {
            return Err(Error::msg(format!("too many records ({})", self.records.len())));
        }
        let store = VdbStore::from_records(self.records.values().cloned().collect())?;
        let bytes = file::save_bytes(&store)?;
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let tmp = self.path.with_extension("history-tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            Error::Io(e)
        })?;
        Ok(())
    }
}

fn empty_header(drawing_id: &str) -> Header {
    Header {
        format: FORMAT.into(),
        version: FORMAT_VERSION,
        drawing_id: drawing_id.into(),
        state: "complete".into(),
        next_milestone: 1,
        milestones: Vec::new(),
    }
}

fn write_member_chunks(records: &mut BTreeMap<u64, VdbRecord>, milestone: u32, store: u16, ids: &[u64]) -> Result<u32, Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut chunks = 0u32;
    for (ci, part) in ids.chunks(MEMBER_CHUNK).enumerate() {
        if ci > 0x0FFF {
            return Err(Error::msg("too many member chunks"));
        }
        let mut payload = Vec::with_capacity(part.len().saturating_mul(8));
        for id in part {
            payload.extend_from_slice(&id.to_le_bytes());
        }
        if payload.len() > MAX_PAYLOAD {
            return Err(Error::msg("member chunk too large"));
        }
        let id = record_id(KIND_MEMBERS, member_n(milestone, store, ci as u16));
        records.insert(id, VdbRecord { id, bbox: bbox_from(ZERO_BOX)?, payload });
        chunks = chunks.saturating_add(1);
    }
    Ok(chunks)
}

fn read_member_chunks(records: &BTreeMap<u64, VdbRecord>, milestone: u32, store: u16, chunks: u32) -> Result<Vec<u64>, Error> {
    if chunks as usize > MAX_STORES.saturating_mul(16) {
        return Err(Error::msg("too many member chunks to read"));
    }
    let mut out = Vec::new();
    for c in 0..chunks {
        let id = record_id(KIND_MEMBERS, member_n(milestone, store, c as u16));
        let rec = records.get(&id).ok_or_else(|| Error::msg(format!("missing member chunk {c}")))?;
        out.extend(decode_member_ids(&rec.payload));
        if out.len() > MAX_RECORDS {
            return Err(Error::msg("member list too long"));
        }
    }
    Ok(out)
}

fn decode_member_ids(payload: &[u8]) -> Vec<u64> {
    let mut out = Vec::with_capacity(payload.len() / 8);
    for chunk in payload.as_chunks::<8>().0 {
        out.push(u64::from_le_bytes(*chunk));
    }
    out
}

/// Path of the history sidecar for a drawing file.
pub fn sidecar_path(drawing_path: &Path) -> PathBuf {
    let mut s = drawing_path.as_os_str().to_os_string();
    s.push(".history.vdb");
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(payload: &[u8], box_: [f64; 4]) -> RowIn {
        RowIn { bbox: box_, payload: payload.to_vec() }
    }

    fn bundle(target: &str, objects: Vec<RowIn>) -> StoreBundle {
        let hash = format!("{:016x}", content_n(&objects.iter().fold(Vec::new(), |mut a, o| {
            a.extend_from_slice(&o.payload);
            a
        })));
        StoreBundle {
            meta: StoreMeta {
                target: target.into(),
                name: target.into(),
                object_count: 0,
                content_hash: hash,
                member_chunks: 0,
                definitions: 0,
            },
            objects,
            definitions: vec![row(br#"{"layer":"0"}"#, ZERO_BOX)],
        }
    }

    #[test]
    fn save_restore_round_trip_and_share_rows() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("d.dxf.history.vdb");
        let mut db = HistoryDb::open(&path, "draw-1").expect("open");
        let a = row(br#"{"h":1}"#, [0.0, 0.0, 1.0, 1.0]);
        let b = row(br#"{"h":2}"#, [2.0, 2.0, 3.0, 3.0]);
        let id1 = db
            .save_milestone(MilestoneIn {
                name: "m1".into(),
                kind: "milestone".into(),
                time: "2026-01-01T00:00:00Z".into(),
                author: "t".into(),
                scope: json!({"type":"project"}),
                parent: None,
                stores: vec![bundle("level:1", vec![a.clone(), b.clone()])],
                layouts: Vec::new(),
            })
            .expect("save1");
        let objects_before = db.records.values().filter(|r| kind_of(r.id) == KIND_OBJECT).count();
        let id2 = db
            .save_milestone(MilestoneIn {
                name: "m2".into(),
                kind: "auto".into(),
                time: "2026-01-02T00:00:00Z".into(),
                author: "t".into(),
                scope: json!({"type":"project"}),
                parent: Some(id1),
                stores: vec![bundle("level:1", vec![a, b])],
                layouts: Vec::new(),
            })
            .expect("save2");
        let objects_after = db.records.values().filter(|r| kind_of(r.id) == KIND_OBJECT).count();
        assert_eq!(objects_before, objects_after, "unchanged objects must be shared");
        assert_ne!(id1, id2);

        let loaded = db.load_milestone(id1).expect("load");
        assert_eq!(loaded.stores.len(), 1);
        assert_eq!(loaded.stores[0].objects.len(), 2);
        assert_eq!(loaded.stores[0].objects[0], br#"{"h":1}"#);

        let renamed = db.rename_milestone(id2, "Named auto").expect("rename");
        assert_eq!(renamed.kind, "milestone");
        assert_eq!(renamed.name, "Named auto");
        assert_eq!(db.list().iter().find(|m| m.id == id2).map(|m| m.kind.as_str()), Some("milestone"));
    }

    #[test]
    fn refuses_foreign_drawing_id() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("d.history.vdb");
        let mut db = HistoryDb::open(&path, "a").expect("open");
        db.save_milestone(MilestoneIn {
            name: "x".into(),
            kind: "milestone".into(),
            time: "2026-01-01T00:00:00Z".into(),
            author: "".into(),
            scope: json!({}),
            parent: None,
            stores: vec![bundle("level:1", vec![row(b"{}", ZERO_BOX)])],
            layouts: Vec::new(),
        })
        .expect("save");
        match HistoryDb::open(&path, "b") {
            Err(e) => assert!(e.to_string().contains("another drawing")),
            Ok(_) => panic!("expected foreign drawing_id refusal"),
        }
    }

    #[test]
    fn hash_collision_probes() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("c.history.vdb");
        let mut db = HistoryDb::open(&path, "d").expect("open");
        // Force two different payloads into the same n by inserting manually then putting.
        let n = 42u64;
        let id = record_id(KIND_OBJECT, n);
        db.records.insert(id, VdbRecord { id, bbox: bbox_from(ZERO_BOX).unwrap(), payload: b"first".to_vec() });
        let placed = db.put_shared(KIND_OBJECT, &row(b"second", ZERO_BOX)).expect("probe");
        assert_ne!(placed, id);
        assert_eq!(db.records.get(&placed).unwrap().payload, b"second");
    }

    #[test]
    fn prune_keeps_recent_autos() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("p.history.vdb");
        let mut db = HistoryDb::open(&path, "d").expect("open");
        for i in 0..5 {
            db.save_milestone(MilestoneIn {
                name: format!("auto-{i}"),
                kind: "auto".into(),
                time: format!("2026-01-0{}T00:00:00Z", i + 1),
                author: "".into(),
                scope: json!({}),
                parent: None,
                stores: vec![bundle("level:1", vec![row(format!("o{i}").as_bytes(), ZERO_BOX)])],
                layouts: Vec::new(),
            })
            .expect("save");
        }
        db.save_milestone(MilestoneIn {
            name: "named".into(),
            kind: "milestone".into(),
            time: "2026-02-01T00:00:00Z".into(),
            author: "".into(),
            scope: json!({}),
            parent: None,
            stores: vec![bundle("level:1", vec![row(b"named", ZERO_BOX)])],
            layouts: Vec::new(),
        })
        .expect("named");
        let dropped = db.prune_auto(2, 0).expect("prune");
        assert!(dropped >= 1);
        assert!(db.list().iter().any(|m| m.kind == "milestone"));
        assert_eq!(db.list().iter().filter(|m| m.kind == "auto").count(), 2);
    }

    #[test]
    fn revision_cannot_be_deleted() {
        let dir = tempfile::tempdir().expect("temp");
        let path = dir.path().join("r.history.vdb");
        let mut db = HistoryDb::open(&path, "d").expect("open");
        let id = db
            .save_milestone(MilestoneIn {
                name: "Rev A".into(),
                kind: "revision".into(),
                time: "2026-01-01T00:00:00Z".into(),
                author: "".into(),
                scope: json!({}),
                parent: None,
                stores: vec![bundle("level:1", vec![row(b"x", ZERO_BOX)])],
                layouts: Vec::new(),
            })
            .expect("save");
        assert!(db.delete_milestone(id).is_err());
    }

    #[test]
    fn sidecar_path_appends_suffix() {
        assert_eq!(sidecar_path(Path::new("C:/a/house.dxf")), PathBuf::from("C:/a/house.dxf.history.vdb"));
    }
}
