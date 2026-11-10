//! The presentation block of an archetype: which effects the unit chooses (plan
//! `2026-11-09-effect-visualization` §5.3).
//!
//! # Responsibility
//! Defines [`ArchetypeEffects`], the optional `effects` block of a unit or structure definition
//! in `assets/catalog/units.ron` and `assets/catalog/structures.ron`. It names, per weapon
//! slot and mode, per mode, per status, per ability and for death, the effect the client draws.
//!
//! # How it fits
//! `sim_core::catalog` parses the block as opaque presentation data, like `veo_prompts`, and
//! keeps it on the archetype only. The client resolves the names against its effect vocabulary
//! (`assets/catalog/effects.ron`) and checks coverage in a test; the simulation never reads it.
//!
//! # Invariants
//! - Never copied onto an `Entity` and never hashed (plan D-1): a block cannot move a checksum.
//! - Every key is optional; an omitted block is the empty block, and unknown keys are refused.
//! - Effect names are data; no Rust names one (charter §3.3).

use serde::{Deserialize, Serialize};
use sim_types::{StatusKind, WeaponSlot};
use std::collections::BTreeMap;

/// The effects one archetype chooses. Empty for an archetype that names none, which then
/// draws the client's defaults.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchetypeEffects {
    /// Effects of each weapon, by slot and mode.
    #[serde(default)]
    pub weapons: Vec<WeaponEffects>,
    /// Effects of each mode's switch and stance.
    #[serde(default)]
    pub modes: Vec<ModeEffects>,
    /// Effect drawn while each status is active.
    #[serde(default)]
    pub status: BTreeMap<StatusKind, String>,
    /// Effect of the area or tether each ability makes, by ability name (plan D-8).
    #[serde(default)]
    pub abilities: BTreeMap<String, String>,
    /// Effect drawn when the entity dies, replacing the default shatter.
    #[serde(default)]
    pub death: Option<String>,
}

impl ArchetypeEffects {
    /// Whether the block names no effect at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.weapons.is_empty()
            && self.modes.is_empty()
            && self.status.is_empty()
            && self.abilities.is_empty()
            && self.death.is_none()
    }
}

/// The effects of one weapon: the archetype's weapon in `weapon`'s slot, or the override of
/// that slot by mode `mode`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeaponEffects {
    /// The slot, matching the archetype's `weapon`, `ground_weapon` or `air_weapon`.
    pub weapon: WeaponSlot,
    /// Name of the mode whose weapon override this is; `None` for the default stance.
    #[serde(default)]
    pub mode: Option<String>,
    /// Effect at the muzzle when the weapon fires.
    #[serde(default)]
    pub launch: Option<String>,
    /// Effect of the shot in flight (a projectile, tracer or beam).
    #[serde(default)]
    pub travel: Option<String>,
    /// Effect where the shot lands.
    #[serde(default)]
    pub impact: Option<String>,
    /// Effect of each splash search, sized by its zones.
    #[serde(default)]
    pub splash: Option<String>,
}

/// The effects of one mode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeEffects {
    /// Name of the mode, as the archetype's `modes` declares it.
    pub mode: String,
    /// Effect played while the unit channels into the mode.
    #[serde(default)]
    pub enter: Option<String>,
    /// Effect played while the unit channels out of the mode.
    #[serde(default)]
    pub leave: Option<String>,
    /// Effect played over either channel, when `enter` and `leave` are not split.
    #[serde(default)]
    pub transition: Option<String>,
    /// Effect held while the unit is in the mode.
    #[serde(default)]
    pub stance: Option<String>,
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn an_omitted_block_is_empty() {
        let block: ArchetypeEffects = ron::from_str("()").expect("an empty block reads");
        assert!(block.is_empty());
        assert_eq!(block, ArchetypeEffects::default());
    }

    #[test]
    fn every_key_of_the_schema_reads() {
        let block: ArchetypeEffects = ron::from_str(
            r#"(
                weapons: [
                    (weapon: Primary, mode: None, launch: Some("a"), impact: Some("b")),
                    (weapon: Air, mode: Some("m"), travel: Some("c"), splash: Some("d")),
                ],
                modes: [(mode: "m", enter: Some("e"), leave: Some("f"), transition: None,
                         stance: Some("g"))],
                status: { Overclock: "h", Barrier: "i" },
                abilities: { "Storm": "j" },
                death: Some("k"),
            )"#,
        )
        .expect("the full schema reads");
        assert!(!block.is_empty());
        assert_eq!(block.weapons.len(), 2);
        assert_eq!(block.weapons[1].weapon, WeaponSlot::Air);
        assert_eq!(block.weapons[1].mode.as_deref(), Some("m"));
        assert_eq!(block.modes[0].stance.as_deref(), Some("g"));
        assert_eq!(
            block.status.get(&StatusKind::Barrier).map(String::as_str),
            Some("i")
        );
        assert_eq!(block.abilities.get("Storm").map(String::as_str), Some("j"));
        assert_eq!(block.death.as_deref(), Some("k"));
    }

    #[test]
    fn an_unknown_key_is_refused() {
        assert!(ron::from_str::<ArchetypeEffects>("(sounds: [])").is_err());
        assert!(ron::from_str::<WeaponEffects>("(weapon: Primary, muzzle: None)").is_err());
    }
}
