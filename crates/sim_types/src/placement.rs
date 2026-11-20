//! Placement flags: the modelled part of `StarCraft` II 5.0.14's footprint flag vocabulary.
//!
//! # Responsibility
//! A 5.0.14 footprint layer either applies flags to the cells it covers (`Place`) or refuses
//! cells that carry them (`Check`). This module names the flags the simulation models, as plain
//! game mechanics (plan `2026-11-19-footprint-flags` D-2), and a small bit set of them:
//! - [`PlacementFlag::NearResources`] is applied by mineral fields and geysers on a ring around
//!   themselves and refused by the town hall analogues' placement footprints.
//! - [`PlacementFlag::Fogged`] is the engine's per-player flag of a cell hidden by the fog of war,
//!   refused by the Spine and Spore Crawler analogues' placement footprint.
//!
//! # How it fits
//! The catalog authors a structure's refused flags (`placement_excludes`) and each resource
//! kind's applied ring by these names; `sim_core`'s site check derives a cell's flags on demand
//! and refuses a site whose cells meet the structure's set. Nothing here is hashed.
//!
//! # Invariants
//! - Only modelled flags exist, so an authored name outside them fails to load (serde's
//!   unknown-variant error) instead of being ignored.
//! - [`PlacementFlag::ALL`] lists the flags in declaration order, which is the order a refusal
//!   reports them in.

use bitflags::bitflags;
use serde::{Deserialize, Serialize};

/// One modelled placement flag, named as in 5.0.14's `FootprintData.xml`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PlacementFlag {
    /// The cell lies within reach of a mineral field or geyser (5.0.14 `NearResources`).
    NearResources,
    /// The cell is hidden from the placing player by the fog of war now (5.0.14 `Fogged`).
    Fogged,
}

impl PlacementFlag {
    /// Every modelled flag, in declaration order.
    pub const ALL: [Self; 2] = [Self::NearResources, Self::Fogged];

    /// The one-flag set holding `self`.
    #[must_use]
    pub const fn bit(self) -> PlacementFlags {
        match self {
            Self::NearResources => PlacementFlags::NEAR_RESOURCES,
            Self::Fogged => PlacementFlags::FOGGED,
        }
    }
}

bitflags! {
    /// A set of [`PlacementFlag`]s: the flags a cell carries, or the flags a structure refuses.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct PlacementFlags: u8 {
        /// [`PlacementFlag::NearResources`].
        const NEAR_RESOURCES = 1 << 0;
        /// [`PlacementFlag::Fogged`].
        const FOGGED = 1 << 1;
    }
}

impl PlacementFlags {
    /// The set holding exactly `flags`; repeated entries count once.
    #[must_use]
    pub fn of(flags: &[PlacementFlag]) -> Self {
        flags
            .iter()
            .fold(Self::empty(), |set, flag| set | flag.bit())
    }

    /// Whether the set holds `flag`.
    #[must_use]
    pub const fn has(self, flag: PlacementFlag) -> bool {
        self.contains(flag.bit())
    }

    /// The lowest flag of the set in declaration order, or `None` when it is empty.
    #[must_use]
    pub fn first(self) -> Option<PlacementFlag> {
        PlacementFlag::ALL.into_iter().find(|flag| self.has(*flag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_flag_has_its_own_bit() {
        let mut seen = PlacementFlags::empty();
        for flag in PlacementFlag::ALL {
            assert!(!seen.intersects(flag.bit()), "{flag:?} shares a bit");
            seen |= flag.bit();
        }
        assert_eq!(seen, PlacementFlags::all());
    }

    #[test]
    fn a_set_reports_its_lowest_flag() {
        assert_eq!(PlacementFlags::empty().first(), None);
        assert_eq!(
            PlacementFlags::all().first(),
            Some(PlacementFlag::NearResources)
        );
        assert_eq!(PlacementFlags::FOGGED.first(), Some(PlacementFlag::Fogged));
    }

    #[test]
    fn a_list_becomes_a_set() {
        let set = PlacementFlags::of(&[PlacementFlag::Fogged, PlacementFlag::Fogged]);
        assert_eq!(set, PlacementFlags::FOGGED);
        assert!(set.has(PlacementFlag::Fogged));
        assert!(!set.has(PlacementFlag::NearResources));
        assert_eq!(PlacementFlags::of(&[]), PlacementFlags::empty());
    }

    #[test]
    fn flags_read_by_their_game_names() -> Result<(), ron::error::SpannedError> {
        let flags: Vec<PlacementFlag> = ron::from_str("[NearResources, Fogged]")?;
        assert_eq!(flags, PlacementFlag::ALL);
        assert!(ron::from_str::<Vec<PlacementFlag>>("[Creep]").is_err());
        Ok(())
    }
}
