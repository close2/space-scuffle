//! Authored movement rules that belong to no single archetype: `assets/catalog/movement_rules.ron`.
//!
//! The file holds the group-routing tolerance of plan `2026-11-10-group-routing` (D-7, sanctioned
//! deviation S-7). It is parsed into [`MovementRules`], validated, and carried by the
//! [`UnitCatalog`](super::UnitCatalog) the world is built with. Like every catalog it is not
//! hashed; lockstep peers must load equal files.
//!
//! Invariant: a [`MovementRules`] value always has a tolerance in `0..=1000` permille.

use crate::error::{SimCoreError, SimResult};
use serde::Deserialize;

/// The largest accepted lane tolerance, in permille (a lane twice as long as the own route).
pub const MAX_LANE_TOLERANCE_PERMILLE: u16 = 1000;

/// Movement rules loaded from `movement_rules.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementRules {
    lane_tolerance_permille: u16,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MovementRulesFile {
    group_routing: GroupRoutingFile,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupRoutingFile {
    lane_tolerance_permille: u32,
}

impl MovementRules {
    /// Per-unit routing, `StarCraft` II's behaviour: tolerance 0, so no command forms a route
    /// group. A [`UnitCatalog`](super::UnitCatalog) built from unit definitions alone carries
    /// these rules until [`UnitCatalog::set_movement_rules`](super::UnitCatalog::set_movement_rules)
    /// replaces them; the runtime loader always reads the file and has no fallback.
    pub const PER_UNIT: Self = Self {
        lane_tolerance_permille: 0,
    };

    /// Rules with the given group-routing tolerance, in permille of the member's own route length.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] when `permille` exceeds
    /// [`MAX_LANE_TOLERANCE_PERMILLE`].
    pub fn with_lane_tolerance(permille: u32) -> SimResult<Self> {
        let lane_tolerance_permille = u16::try_from(permille)
            .ok()
            .filter(|value| *value <= MAX_LANE_TOLERANCE_PERMILLE)
            .ok_or_else(|| SimCoreError::CatalogInvalidValue {
                definition: "group_routing".to_owned(),
                field: "lane_tolerance_permille".to_owned(),
                reason: "must be between 0 and 1000 permille",
            })?;
        Ok(Self {
            lane_tolerance_permille,
        })
    }

    /// Parses and validates the text of `movement_rules.ron`.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON or unknown or missing fields,
    /// and [`SimCoreError::CatalogInvalidValue`] for a tolerance above 1000 permille.
    pub fn from_ron_str(ron_data: &str) -> SimResult<Self> {
        let file: MovementRulesFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;
        Self::with_lane_tolerance(file.group_routing.lane_tolerance_permille)
    }

    /// The group-routing tolerance in permille: a member adopts its group's lane when the lane
    /// route is at most this many thousandths longer than its own route. 0 disables groups.
    #[must_use]
    pub const fn lane_tolerance_permille(self) -> u16 {
        self.lane_tolerance_permille
    }

    /// Whether multi-unit movement commands form route groups (tolerance above 0).
    #[must_use]
    pub const fn groups_enabled(self) -> bool {
        self.lane_tolerance_permille > 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_file_parses() -> SimResult<()> {
        let rules = MovementRules::from_ron_str(include_str!(
            "../../../../assets/catalog/movement_rules.ron"
        ))?;
        assert!(rules.lane_tolerance_permille() <= MAX_LANE_TOLERANCE_PERMILLE);
        Ok(())
    }

    #[test]
    fn bounds_are_inclusive() -> SimResult<()> {
        assert_eq!(
            MovementRules::with_lane_tolerance(0)?,
            MovementRules::PER_UNIT
        );
        assert!(!MovementRules::PER_UNIT.groups_enabled());
        let most = MovementRules::with_lane_tolerance(1000)?;
        assert_eq!(most.lane_tolerance_permille(), 1000);
        assert!(most.groups_enabled());
        assert!(matches!(
            MovementRules::with_lane_tolerance(1001),
            Err(SimCoreError::CatalogInvalidValue { .. })
        ));
        assert!(MovementRules::with_lane_tolerance(u32::MAX).is_err());
        Ok(())
    }

    #[test]
    fn malformed_text_and_unknown_or_missing_fields_are_rejected() {
        for text in [
            "(",
            "(group_routing: (lane_tolerance_permille: -1))",
            "(group_routing: (lane_tolerance_permille: 75, extra: 1))",
            "(group_routing: ())",
            "()",
        ] {
            assert!(
                matches!(
                    MovementRules::from_ron_str(text),
                    Err(SimCoreError::RonDeserializationError(_))
                ),
                "{text}"
            );
        }
    }
}
