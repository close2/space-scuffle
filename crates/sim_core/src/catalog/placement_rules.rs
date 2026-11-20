//! Authored placement rules that belong to no single archetype: `assets/catalog/placement_rules.ron`.
//!
//! # Responsibility
//! The file holds, per resource kind, the cells around a node on which the node applies
//! placement flags: 5.0.14's `NearResources` ring of `FootprintMineralsRounded` and
//! `FootprintGeyserRounded` (plan `2026-11-19-footprint-flags` D-2, D-3). Each ring is authored as
//! a character mask, `x` for a cell in the ring and `.` for one outside it, with the node's own
//! cell named by `anchor`. Loading parses the masks once into sorted cell offsets
//! ([`ResourceReach`]); the site check reads them without allocating.
//!
//! # How it fits
//! [`RuntimeCatalogs::load_from_directory`](super::RuntimeCatalogs::load_from_directory) reads the
//! file beside `movement_rules.ron` and stores the result in the [`UnitCatalog`](super::UnitCatalog)
//! ([`UnitCatalog::placement_rules`](super::UnitCatalog::placement_rules)). A catalog built from
//! unit and structure text alone carries [`PlacementRules::NONE`], under which no resource applies
//! a flag. Like every catalog it is not hashed; lockstep peers must load equal files.
//!
//! # Invariants
//! - Each resource kind has at most one reach, holding its node's own cell.
//! - A reach applies at least one flag and never [`PlacementFlag::Fogged`], an engine state flag.
//! - Offsets are `(dx, dy)` in grid cells from the node's cell, `dy` growing with world `y`, sorted
//!   by `dy` then `dx`, without repeats.

use crate::error::{SimCoreError, SimResult};
use serde::Deserialize;
use sim_types::{PlacementFlag, PlacementFlags, ResourceKind};

/// The largest side of an authored mask, in cells.
pub const MAX_REACH_SIDE: usize = 15;

/// The cells around one kind of resource node on which it applies placement flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceReach {
    /// The kind of node this reach belongs to.
    pub kind: ResourceKind,
    /// The flags the node applies on every cell of the reach.
    pub applies: PlacementFlags,
    /// The reach's cells as `(dx, dy)` offsets in grid cells from the node's cell, sorted by
    /// `dy` then `dx`; `(0, 0)` is always present.
    pub cells: Vec<(i8, i8)>,
}

impl ResourceReach {
    /// Whether the cell `(dx, dy)` grid cells from the node's cell lies in this reach.
    #[must_use]
    pub fn covers(&self, dx: i64, dy: i64) -> bool {
        let (Ok(dx), Ok(dy)) = (i8::try_from(dx), i8::try_from(dy)) else {
            return false;
        };
        self.cells
            .binary_search_by(|&(x, y)| (y, x).cmp(&(dy, dx)))
            .is_ok()
    }
}

/// The placement rules loaded from `placement_rules.ron`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlacementRules {
    reaches: Vec<ResourceReach>,
}

/// The file's top level.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PlacementRulesFile {
    pub(super) resource_footprints: Vec<ResourceFootprintDef>,
}

/// One authored resource ring.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResourceFootprintDef {
    /// The node kind the ring belongs to.
    pub(super) kind: ResourceKind,
    /// The flags the ring applies.
    pub(super) applies: Vec<PlacementFlag>,
    /// `(column, row)` of the node's own cell in `rows`.
    pub(super) anchor: (u32, u32),
    /// The mask; `rows[0]` is the row of lowest `y`.
    pub(super) rows: Vec<String>,
}

impl PlacementRules {
    /// No resource applies any flag: the rules of a catalog that has not loaded the file.
    pub const NONE: Self = Self {
        reaches: Vec::new(),
    };

    /// Parses and validates the text of `placement_rules.ron`.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON, unknown or missing fields and
    /// unknown flag or kind names; [`SimCoreError::CatalogInvalidValue`] for a mask or flag list
    /// that breaks the module's invariants.
    pub fn from_ron_str(ron_data: &str) -> SimResult<Self> {
        let file: PlacementRulesFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;
        file.validate()?;
        let reaches = file
            .resource_footprints
            .iter()
            .map(ResourceFootprintDef::to_reach)
            .collect::<SimResult<Vec<_>>>()?;
        Ok(Self { reaches })
    }

    /// The reach of nodes of `kind`, or `None` when that kind applies nothing.
    #[must_use]
    pub fn reach(&self, kind: ResourceKind) -> Option<&ResourceReach> {
        self.reaches.iter().find(|reach| reach.kind == kind)
    }

    /// Every authored reach, in the file's order.
    #[must_use]
    pub fn reaches(&self) -> &[ResourceReach] {
        &self.reaches
    }
}

impl ResourceFootprintDef {
    /// The validated mask as sorted offsets from the anchor.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] for an offset outside `i8`, which validation's bound
    /// of [`MAX_REACH_SIDE`] cells a side already excludes.
    fn to_reach(&self) -> SimResult<ResourceReach> {
        let offset = |at: usize, anchor: u32| {
            let at = i64::try_from(at).ok();
            at.and_then(|at| i8::try_from(at - i64::from(anchor)).ok())
                .ok_or_else(|| SimCoreError::CatalogInvalidValue {
                    definition: format!("{:?}", self.kind),
                    field: "rows".to_owned(),
                    reason: "must be at most 15 cells a side",
                })
        };
        let mut cells = Vec::new();
        for (row, text) in self.rows.iter().enumerate() {
            for (column, mark) in text.chars().enumerate() {
                if mark == 'x' {
                    cells.push((offset(column, self.anchor.0)?, offset(row, self.anchor.1)?));
                }
            }
        }
        cells.sort_by_key(|&(dx, dy)| (dy, dx));
        Ok(ResourceReach {
            kind: self.kind,
            applies: PlacementFlags::of(&self.applies),
            cells,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED: &str = include_str!("../../../../assets/catalog/placement_rules.ron");

    #[test]
    fn the_shipped_file_parses() -> SimResult<()> {
        let rules = PlacementRules::from_ron_str(SHIPPED)?;
        assert!(rules.reach(ResourceKind::Mineral).is_some());
        assert!(rules.reach(ResourceKind::Gas).is_some());
        Ok(())
    }

    #[test]
    fn offsets_are_measured_from_the_anchor() -> SimResult<()> {
        let rules = PlacementRules::from_ron_str(
            r#"(resource_footprints: [
                (kind: Mineral, applies: [NearResources], anchor: (1, 0), rows: ["xx.", ".x."]),
            ])"#,
        )?;
        let reach = rules
            .reach(ResourceKind::Mineral)
            .ok_or(SimCoreError::IoError(
                "the mineral reach was loaded".to_owned(),
            ))?;
        assert_eq!(reach.cells, vec![(-1, 0), (0, 0), (0, 1)]);
        assert!(reach.covers(0, 1));
        assert!(!reach.covers(1, 0));
        assert!(!reach.covers(1_000, 0));
        assert_eq!(reach.applies, PlacementFlags::NEAR_RESOURCES);
        assert!(rules.reach(ResourceKind::Gas).is_none());
        Ok(())
    }

    #[test]
    fn no_rules_apply_nothing() {
        assert_eq!(PlacementRules::NONE.reaches(), &[]);
        assert_eq!(PlacementRules::default(), PlacementRules::NONE);
    }
}
