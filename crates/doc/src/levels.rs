//! Levels: named model spaces that share one origin and one layer table.
//!
//! `Drawing::model` always holds the store being edited (the active level's); every other level's
//! store waits in its `Level::entities` slot. `Drawing::switch_level` is the only place that swaps
//! them, so commands keep writing to `model` and the invariant lives here.
//!
//! A drawing with no `Level` records is a single implicit level (every older file): nothing changes
//! for it until the first level is added with `Drawing::materialize_levels`.

use aad_geom::Vec2;

use crate::{DocError, Drawing, EntityStore, Result};

/// Most levels in one drawing.
pub const MAX_LEVELS: usize = 64;
/// Most traces on one level.
pub const MAX_TRACES: usize = 8;
/// Longest level name, in characters.
pub const MAX_LEVEL_NAME: usize = 64;
/// Project-data key a file reader sets when level data in a file was damaged or incomplete; the
/// open command reports its text on the command line and removes it.
pub const LEVELS_NOTE_KEY: &str = "levelsNote";

/// Stable level identity: never reused, names may change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize)]
pub struct LevelId(pub u32);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TraceColour {
    #[default]
    Grey,
    TrueColour,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TraceOrder {
    #[default]
    Below,
    Above,
}

/// A read-only faded display of another level under the one being edited.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Trace {
    pub level: LevelId,
    /// 0..=90 percent.
    pub fade: u8,
    pub colour: TraceColour,
    pub order: TraceOrder,
    /// Object snaps may use this trace.
    pub snap: bool,
    /// `None` = every visible layer.
    pub layers: Option<Vec<String>>,
}

impl Trace {
    pub fn new(level: LevelId) -> Self {
        Trace { level, fade: 60, colour: TraceColour::Grey, order: TraceOrder::Below, snap: false, layers: None }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Level {
    pub id: LevelId,
    /// 1..=64 characters, unique (case-insensitive).
    pub name: String,
    pub elevation: f64,
    pub height: f64,
    pub traces: Vec<Trace>,
    /// Last view (centre, height) on this level.
    pub view: Option<(Vec2, f64)>,
    /// The level's objects while it is not the active level; empty while it is in `Drawing::model`.
    pub entities: EntityStore,
}

/// Whether `name` is usable as a level name.
pub fn level_name_ok(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.chars().count() <= MAX_LEVEL_NAME && !n.chars().any(char::is_control)
}

fn invalid(msg: impl Into<String>) -> DocError {
    DocError::Invalid(msg.into())
}

impl Drawing {
    /// The level with this id.
    pub fn level(&self, id: LevelId) -> Option<&Level> {
        self.levels.iter().find(|l| l.id == id)
    }

    /// The level with this name (case-insensitive).
    pub fn level_by_name(&self, name: &str) -> Option<&Level> {
        self.levels.iter().find(|l| l.name.eq_ignore_ascii_case(name.trim()))
    }

    /// Whether the drawing has explicit levels (otherwise it is one implicit level).
    pub fn has_levels(&self) -> bool {
        !self.levels.is_empty()
    }

    /// The level being edited, if the drawing has explicit levels.
    pub fn active_level_ref(&self) -> Option<&Level> {
        self.active_level.and_then(|id| self.level(id))
    }

    /// Every entity store that belongs to model space: the active one (`model`) first, then the
    /// stores of the other levels. For drawing-wide work (purge, audit, extraction).
    pub fn model_stores(&self) -> impl Iterator<Item = &EntityStore> + '_ {
        // The slot of whatever is in `model` is empty, so every slot can be listed.
        std::iter::once(&self.model).chain(self.levels.iter().map(|l| &l.entities)).chain(self.sheet_sets.iter().flat_map(|s| s.annotations.values()))
    }

    /// Mutable access to every model-space store (the active one first), for drawing-wide edits
    /// such as renaming a layer.
    pub fn model_stores_mut(&mut self) -> impl Iterator<Item = &mut EntityStore> + '_ {
        std::iter::once(&mut self.model)
            .chain(self.levels.iter_mut().map(|l| &mut l.entities))
            .chain(self.sheet_sets.iter_mut().flat_map(|s| s.annotations.values_mut()))
    }

    /// The objects of a level, whether or not it is the active one.
    pub fn level_store(&self, id: LevelId) -> Option<&EntityStore> {
        let l = self.level(id)?;
        Some(if self.target() == Some(crate::EditTarget::Level(id)) { &self.model } else { &l.entities })
    }

    fn check_unique(&self, name: &str, except: Option<LevelId>) -> Result<()> {
        if !level_name_ok(name) {
            return Err(invalid(format!("a level name is 1 to {MAX_LEVEL_NAME} characters without control characters")));
        }
        if self.levels.iter().any(|l| Some(l.id) != except && l.name.eq_ignore_ascii_case(name.trim())) {
            return Err(invalid(format!("there is already a level named `{}`", name.trim())));
        }
        Ok(())
    }

    /// Turn the implicit single level into an explicit one named `first_name` that owns `model`.
    /// Returns the active level's id unchanged when the drawing already has levels.
    pub fn materialize_levels(&mut self, first_name: &str, elevation: f64, height: f64) -> Result<LevelId> {
        if let Some(id) = self.active_level {
            return Ok(id);
        }
        if !elevation.is_finite() || !height.is_finite() || height < 0.0 {
            return Err(invalid("elevation and height must be finite numbers, the height not negative"));
        }
        self.check_unique(first_name, None)?;
        self.level_seed = self.level_seed.saturating_add(1);
        let id = LevelId(self.level_seed);
        self.levels.push(Level {
            id,
            name: first_name.trim().to_string(),
            elevation,
            height,
            traces: Vec::new(),
            view: None,
            entities: EntityStore::new(),
        });
        self.active_level = Some(id);
        Ok(id)
    }

    /// Add an empty level after `after` (else at the end). The drawing must already have levels.
    pub fn add_level(&mut self, name: &str, elevation: f64, height: f64, after: Option<LevelId>) -> Result<LevelId> {
        if self.levels.is_empty() {
            return Err(invalid("the drawing has no explicit levels yet"));
        }
        if self.levels.len() >= MAX_LEVELS {
            return Err(invalid(format!("at most {MAX_LEVELS} levels")));
        }
        if !elevation.is_finite() || !height.is_finite() || height < 0.0 {
            return Err(invalid("elevation and height must be finite numbers, the height not negative"));
        }
        self.check_unique(name, None)?;
        let at = match after {
            Some(a) => self.levels.iter().position(|l| l.id == a).ok_or_else(|| invalid("no such level to insert after"))? + 1,
            None => self.levels.len(),
        };
        self.level_seed = self.level_seed.saturating_add(1);
        let id = LevelId(self.level_seed);
        self.levels
            .insert(at, Level { id, name: name.trim().to_string(), elevation, height, traces: Vec::new(), view: None, entities: EntityStore::new() });
        Ok(id)
    }

    /// Rename a level.
    pub fn rename_level(&mut self, id: LevelId, name: &str) -> Result<()> {
        self.check_unique(name, Some(id))?;
        let l = self.levels.iter_mut().find(|l| l.id == id).ok_or_else(|| invalid("no such level"))?;
        l.name = name.trim().to_string();
        Ok(())
    }

    /// Make `id` the level being edited: the current level's objects go back to its slot and the
    /// new level's objects become `model`.
    pub fn switch_level(&mut self, id: LevelId) -> Result<()> {
        // In a sheet view the sheet set stays: the same discipline on another level.
        let to = match self.active_sheet {
            Some(s) => crate::EditTarget::Sheet(s, id),
            None => crate::EditTarget::Level(id),
        };
        self.switch_target(to)
    }

    /// Remove a level and its objects, and every trace of it. The last level cannot be removed;
    /// removing the active level activates its neighbour first.
    pub fn remove_level(&mut self, id: LevelId) -> Result<()> {
        let pos = self.levels.iter().position(|l| l.id == id).ok_or_else(|| invalid("no such level"))?;
        if self.levels.len() <= 1 {
            return Err(invalid("the last level cannot be deleted"));
        }
        if self.active_level == Some(id) {
            let next = if pos + 1 < self.levels.len() { pos + 1 } else { pos.saturating_sub(1) };
            if let Some(next_id) = self.levels.get(next).map(|l| l.id) {
                self.switch_level(next_id)?;
            }
        }
        self.levels.retain(|l| l.id != id);
        for l in &mut self.levels {
            l.traces.retain(|t| t.level != id);
        }
        for s in &mut self.sheet_sets {
            s.annotations.remove(&id);
        }
        Ok(())
    }

    /// Move a level to `index` in tab order.
    pub fn move_level(&mut self, id: LevelId, index: usize) -> Result<()> {
        let pos = self.levels.iter().position(|l| l.id == id).ok_or_else(|| invalid("no such level"))?;
        let l = self.levels.remove(pos);
        let at = index.min(self.levels.len());
        self.levels.insert(at, l);
        Ok(())
    }

    /// Add a trace of `trace.level` to `on`, or replace the existing trace of that level.
    /// A level cannot trace itself, and one level has at most [`MAX_TRACES`] traces.
    pub fn set_trace(&mut self, on: LevelId, trace: Trace) -> Result<()> {
        if on == trace.level {
            return Err(invalid("a level cannot trace itself"));
        }
        if trace.fade > 90 {
            return Err(invalid("fade is 0 to 90 percent"));
        }
        if self.level(trace.level).is_none() {
            return Err(invalid("no such level to trace"));
        }
        let l = self.levels.iter_mut().find(|l| l.id == on).ok_or_else(|| invalid("no such level"))?;
        if let Some(t) = l.traces.iter_mut().find(|t| t.level == trace.level) {
            *t = trace;
            return Ok(());
        }
        if l.traces.len() >= MAX_TRACES {
            return Err(invalid(format!("at most {MAX_TRACES} traces on a level")));
        }
        l.traces.push(trace);
        Ok(())
    }

    /// Remove the trace of `of` from `on`; returns whether there was one.
    pub fn remove_trace(&mut self, on: LevelId, of: LevelId) -> bool {
        let Some(l) = self.levels.iter_mut().find(|l| l.id == on) else { return false };
        let n = l.traces.len();
        l.traces.retain(|t| t.level != of);
        l.traces.len() != n
    }

    /// Whether any other level traces `id`.
    pub fn level_is_traced(&self, id: LevelId) -> bool {
        self.levels.iter().any(|l| l.id != id && l.traces.iter().any(|t| t.level == id))
    }
}
