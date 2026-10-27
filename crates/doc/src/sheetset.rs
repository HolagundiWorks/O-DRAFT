//! Sheet sets: named disciplines (Working drawing, Electrical, Plumbing, ...) that add annotation
//! on top of levels (design: `docs/levels-and-sheet-sets.md`).
//!
//! A sheet set keeps one annotation store per level. In a *sheet view* (a sheet set and a level)
//! `Drawing::model` holds that annotation store, so every drawing command writes annotation, and
//! the level's own objects wait in the level's slot as a read-only reference. Design mode (no
//! sheet set) keeps the level's objects in `model` as before. `Drawing::switch_target` is the only
//! place that swaps stores.

use std::collections::BTreeMap;

use crate::levels::{LevelId, level_name_ok};
use crate::{DocError, Drawing, EntityStore, Result};

/// Most sheet sets in one drawing.
pub const MAX_SHEET_SETS: usize = 32;
/// Longest layer prefix of a sheet set, in characters.
pub const MAX_LAYER_PREFIX: usize = 8;

/// Stable sheet-set identity: never reused.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct SheetSetId(pub u32);

/// Whose objects are in `Drawing::model`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTarget {
    /// A level's design objects.
    Level(LevelId),
    /// A sheet set's annotation on a level.
    Sheet(SheetSetId, LevelId),
}

#[derive(Clone, Debug, PartialEq)]
pub struct SheetSet {
    pub id: SheetSetId,
    /// 1..=64 characters, unique (case-insensitive).
    pub name: String,
    /// New annotation goes on layers that start with this (`E-` gives `E-DIM`, `E-TEXT`, ...).
    pub layer_prefix: String,
    /// The drawing space (saved layer state) that decides which level layers show, if any.
    pub space: Option<String>,
    pub dim_style: Option<String>,
    pub text_style: Option<String>,
    /// Annotation per level while it is not the one in `Drawing::model`.
    pub annotations: BTreeMap<LevelId, EntityStore>,
}

fn invalid(msg: impl Into<String>) -> DocError {
    DocError::Invalid(msg.into())
}

/// Whether `prefix` is usable as a layer prefix (empty is allowed).
pub fn layer_prefix_ok(prefix: &str) -> bool {
    prefix.chars().count() <= MAX_LAYER_PREFIX && !prefix.chars().any(|c| c.is_control() || c.is_whitespace())
}

impl Drawing {
    pub fn sheet_set(&self, id: SheetSetId) -> Option<&SheetSet> {
        self.sheet_sets.iter().find(|s| s.id == id)
    }

    pub fn sheet_set_by_name(&self, name: &str) -> Option<&SheetSet> {
        self.sheet_sets.iter().find(|s| s.name.eq_ignore_ascii_case(name.trim()))
    }

    /// The sheet set of the sheet view being edited, if any.
    pub fn active_sheet_ref(&self) -> Option<&SheetSet> {
        self.active_sheet.and_then(|id| self.sheet_set(id))
    }

    /// Whose objects are in `model` now (`None` while the drawing has no explicit levels).
    pub fn target(&self) -> Option<EditTarget> {
        let level = self.active_level?;
        Some(match self.active_sheet {
            Some(s) => EditTarget::Sheet(s, level),
            None => EditTarget::Level(level),
        })
    }

    /// The level's own objects while a sheet view shows them for reference (read-only).
    pub fn reference_store(&self) -> Option<&EntityStore> {
        self.active_sheet?;
        self.level(self.active_level?).map(|l| &l.entities)
    }

    /// A sheet set's annotation of a level, wherever it is stored now.
    pub fn annotation_store(&self, set: SheetSetId, level: LevelId) -> Option<&EntityStore> {
        let s = self.sheet_set(set)?;
        if self.target() == Some(EditTarget::Sheet(set, level)) {
            return Some(&self.model);
        }
        s.annotations.get(&level)
    }

    fn slot_mut(&mut self, t: EditTarget) -> Option<&mut EntityStore> {
        match t {
            EditTarget::Level(id) => self.levels.iter_mut().find(|l| l.id == id).map(|l| &mut l.entities),
            EditTarget::Sheet(set, level) => self.sheet_sets.iter_mut().find(|s| s.id == set).map(|s| s.annotations.entry(level).or_default()),
        }
    }

    /// Make `to` the store being edited: the current store goes back to its slot and the new one
    /// becomes `model`.
    pub fn switch_target(&mut self, to: EditTarget) -> Result<()> {
        let (set, level) = match to {
            EditTarget::Level(l) => (None, l),
            EditTarget::Sheet(s, l) => (Some(s), l),
        };
        if self.level(level).is_none() {
            return Err(invalid("no such level"));
        }
        if set.is_some_and(|s| self.sheet_set(s).is_none()) {
            return Err(invalid("no such sheet set"));
        }
        if self.target() == Some(to) {
            return Ok(());
        }
        if let Some(cur) = self.target() {
            let parked = std::mem::take(&mut self.model);
            match self.slot_mut(cur) {
                Some(slot) => *slot = parked,
                None => {
                    self.model = parked;
                    return Err(invalid("the store being edited has no slot"));
                }
            }
        }
        self.model = self.slot_mut(to).map(std::mem::take).unwrap_or_default();
        self.active_level = Some(level);
        self.active_sheet = set;
        Ok(())
    }

    /// Add a sheet set; returns its id.
    pub fn add_sheet_set(&mut self, name: &str, layer_prefix: &str) -> Result<SheetSetId> {
        if self.sheet_sets.len() >= MAX_SHEET_SETS {
            return Err(invalid(format!("at most {MAX_SHEET_SETS} sheet sets")));
        }
        self.check_sheet_name(name, None)?;
        if !layer_prefix_ok(layer_prefix) {
            return Err(invalid(format!("a layer prefix is at most {MAX_LAYER_PREFIX} characters without spaces")));
        }
        self.sheet_seed = self.sheet_seed.saturating_add(1);
        let id = SheetSetId(self.sheet_seed);
        self.sheet_sets.push(SheetSet {
            id,
            name: name.trim().to_string(),
            layer_prefix: layer_prefix.to_string(),
            space: None,
            dim_style: None,
            text_style: None,
            annotations: BTreeMap::new(),
        });
        Ok(id)
    }

    fn check_sheet_name(&self, name: &str, except: Option<SheetSetId>) -> Result<()> {
        if !level_name_ok(name) {
            return Err(invalid("a sheet set name is 1 to 64 characters without control characters"));
        }
        if self.sheet_sets.iter().any(|s| Some(s.id) != except && s.name.eq_ignore_ascii_case(name.trim())) {
            return Err(invalid(format!("there is already a sheet set named `{}`", name.trim())));
        }
        Ok(())
    }

    /// Rename a sheet set.
    pub fn rename_sheet_set(&mut self, id: SheetSetId, name: &str) -> Result<()> {
        self.check_sheet_name(name, Some(id))?;
        let s = self.sheet_sets.iter_mut().find(|s| s.id == id).ok_or_else(|| invalid("no such sheet set"))?;
        s.name = name.trim().to_string();
        Ok(())
    }

    /// Remove a sheet set and all its annotation. Leaves the sheet view first when it is the active one.
    pub fn remove_sheet_set(&mut self, id: SheetSetId) -> Result<()> {
        if self.sheet_set(id).is_none() {
            return Err(invalid("no such sheet set"));
        }
        if self.active_sheet == Some(id) {
            self.leave_sheet_view()?;
        }
        self.sheet_sets.retain(|s| s.id != id);
        Ok(())
    }

    /// Show `id` for the active level: its annotation becomes the store being edited and the level
    /// becomes a read-only reference.
    pub fn enter_sheet_view(&mut self, id: SheetSetId) -> Result<()> {
        let level = self.active_level.ok_or_else(|| invalid("the drawing has no explicit levels yet"))?;
        self.switch_target(EditTarget::Sheet(id, level))
    }

    /// Back to design mode on the active level.
    pub fn leave_sheet_view(&mut self) -> Result<()> {
        match self.active_level {
            Some(level) if self.active_sheet.is_some() => self.switch_target(EditTarget::Level(level)),
            _ => Ok(()),
        }
    }
}
