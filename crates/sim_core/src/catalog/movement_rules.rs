//! Authored movement rules that belong to no single archetype: `assets/catalog/movement_rules.ron`.
//!
//! The file holds the group-routing tolerance of plan `2026-11-10-group-routing` (D-7, sanctioned
//! deviation S-7), the leash of an automatic chase (5.0.14's `AcquireLeashRadius`, plan
//! `2026-11-18-contour-footprints` Amendment 4, B-142) and the contact rules of plan
//! `2026-11-20-contact-solver` (§3.6): which movement pipeline runs, the contact solver's
//! iteration count, its dead band and moving-overlap bound (sanctioned deviation D-α), the side
//! rule's horizon factor, the D-β switch, and the crowd extension C-3′ of plan
//! `2026-11-26-friend-crowd-overlap` (`crowd_iterations`, `crowd_depth`). It is parsed into [`MovementRules`], validated, and
//! carried by the [`UnitCatalog`](super::UnitCatalog) the world is built with. Like every catalog
//! it is not hashed; lockstep peers must load equal files.
//!
//! Invariants: a [`MovementRules`] value always has a tolerance in `0..=1000` permille, a leash
//! radius of at least one world unit, and [`ContactRules`] with at least one iteration,
//! `0 < dead_band_permille < moving_overlap_permille < 1000`, a horizon factor of at least 1,
//! `iterations ≤ crowd_iterations ≤ 1000` and a crowd depth of at least 1.

use crate::error::{SimCoreError, SimResult};
use serde::Deserialize;
use sim_types::SimScalar;

/// The largest accepted lane tolerance, in permille (a lane twice as long as the own route).
pub const MAX_LANE_TOLERANCE_PERMILLE: u16 = 1000;

/// 5.0.14's leash of an automatically acquired chase, in world units: `CGame`
/// `AcquireLeashRadius` 21 (core `GameData.xml`:9 at `5444e12f13`). The rules a catalog is built
/// with before `movement_rules.ron` replaces them carry it; the file authors the shipped value.
pub const STARCRAFT_ACQUIRE_LEASH_RADIUS: u16 = 21;

/// The most averaged iterations per tick C-3′ may run (plan `2026-11-26-friend-crowd-overlap`
/// §4.1): a bound on the cost of one tick, far above any derived value.
pub const MAX_CROWD_ITERATIONS: u16 = 1000;

/// C-3′a's reference crowd depth, in bodies (plan `2026-11-26-friend-crowd-overlap` §4.1): the
/// throughput reference army of 400 units packed as a disc against a front, `⌈√(400 / π)⌉ = 12`.
/// The depth a file that omits `crowd_depth` reads.
pub const REFERENCE_CROWD_DEPTH: u16 = 12;

/// Which local movement pipeline the tick runs (plan `2026-11-20-contact-solver` §3.6, a
/// migration switch until the legacy pipeline is deleted).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum MovementPipeline {
    /// Today's pipeline: steering with its proposal-time velocity limit, the waiting rule, the
    /// surround walk and the separate soft collision pass.
    Legacy,
    /// The plan's pipeline: intent steering with one time-to-collision side rule, and one
    /// position-based contact solver after every unit moved.
    Solver,
}

/// The contact rules of `movement_rules.ron` (plan `2026-11-20-contact-solver` §3.3, §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(from = "ContactRulesFile")]
pub struct ContactRules {
    /// The pipeline the tick runs.
    pub pipeline: MovementPipeline,
    /// C-3 (plan Amendment 1): Jacobi iterations of the contact solver per tick, the smallest
    /// count that passes the solver's generated-crowd property for the catalog's worst injected
    /// overlap `ρ`; never below the closed-form bound `⌈ln(moving / ρ) / ln(5/6)⌉`.
    pub iterations: u8,
    /// C-4, D-α at rest: an overlap of at most this many thousandths of the radius sum is not
    /// corrected.
    pub dead_band_permille: u16,
    /// D-α while moving: the bound, in thousandths of the radius sum, that `iterations` is
    /// derived for.
    pub moving_overlap_permille: u16,
    /// I-3a: a threat counts from `factor · (r + r_b) / speed` ticks before contact (2: at most a
    /// 30° turn).
    pub side_horizon_factor: u8,
    /// D-β: moving friends push each other by priority (`true`), or each yields its own approach
    /// (`false`).
    pub moving_friends_push: bool,
    /// Deviation D-δ (Q0095, Q0052 addendum): allies may nudge an engaged melee unit along its
    /// attack ring, never out of range, never by enemies, never on Hold Position. On when absent.
    pub nudge_engaged_along_ring: bool,
    /// C-3′ (plan `2026-11-26-friend-crowd-overlap` §4.1): the most averaged iterations per tick
    /// in all. Past `iterations`, the solver keeps iterating while a friend pair overlaps by more
    /// than `moving_overlap_permille` and the friends' summed excess over it still falls. Equal
    /// to `iterations` switches the extension off; a file that omits it reads `iterations`.
    pub crowd_iterations: u16,
    /// C-3′a: the depth, in bodies, of the deepest crowd behind a front for which
    /// `crowd_iterations` is derived; [`REFERENCE_CROWD_DEPTH`] when the file omits it. Read by
    /// the derivation test only.
    pub crowd_depth: u16,
}

/// The contact section as `movement_rules.ron` writes it: `nudge_engaged_along_ring`,
/// `crowd_iterations` and `crowd_depth` may be absent.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContactRulesFile {
    pipeline: MovementPipeline,
    iterations: u8,
    dead_band_permille: u16,
    moving_overlap_permille: u16,
    side_horizon_factor: u8,
    moving_friends_push: bool,
    #[serde(default = "nudge_default")]
    nudge_engaged_along_ring: bool,
    #[serde(default, deserialize_with = "present")]
    crowd_iterations: Option<u16>,
    #[serde(default = "crowd_depth_default")]
    crowd_depth: u16,
}

impl From<ContactRulesFile> for ContactRules {
    fn from(file: ContactRulesFile) -> Self {
        Self {
            pipeline: file.pipeline,
            iterations: file.iterations,
            dead_band_permille: file.dead_band_permille,
            moving_overlap_permille: file.moving_overlap_permille,
            side_horizon_factor: file.side_horizon_factor,
            moving_friends_push: file.moving_friends_push,
            nudge_engaged_along_ring: file.nudge_engaged_along_ring,
            crowd_iterations: file.crowd_iterations.unwrap_or(u16::from(file.iterations)),
            crowd_depth: file.crowd_depth,
        }
    }
}

/// D-δ is on unless the file switches it off (Q0095).
const fn nudge_default() -> bool {
    true
}

/// A field the file writes as a plain value, `None` only when it is absent (`serde(default)`).
fn present<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<u16>, D::Error> {
    u16::deserialize(deserializer).map(Some)
}

/// C-3′a's depth when the file omits it.
const fn crowd_depth_default() -> u16 {
    REFERENCE_CROWD_DEPTH
}

impl ContactRules {
    /// The rules a catalog carries before `movement_rules.ron` replaces them: the legacy
    /// pipeline, with the plan's derived values for the solver.
    pub const LEGACY: Self = Self {
        pipeline: MovementPipeline::Legacy,
        iterations: 21,
        dead_band_permille: 10,
        moving_overlap_permille: 50,
        side_horizon_factor: 2,
        moving_friends_push: true,
        nudge_engaged_along_ring: true,
        crowd_iterations: 21,
        crowd_depth: REFERENCE_CROWD_DEPTH,
    };

    /// Whether C-3′ is switched on: `crowd_iterations` above `iterations`.
    #[must_use]
    pub fn crowd_extension_on(&self) -> bool {
        self.crowd_iterations > u16::from(self.iterations)
    }

    /// These rules if they are valid (§3.6).
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] naming the first field out of range.
    pub fn validated(self) -> SimResult<Self> {
        let invalid = |field: &str, reason| SimCoreError::CatalogInvalidValue {
            definition: "contact".to_owned(),
            field: field.to_owned(),
            reason,
        };
        if self.iterations == 0 {
            return Err(invalid("iterations", "must be at least 1"));
        }
        if self.dead_band_permille == 0 || self.dead_band_permille >= self.moving_overlap_permille {
            return Err(invalid(
                "dead_band_permille",
                "must be above 0 and below moving_overlap_permille",
            ));
        }
        if self.moving_overlap_permille >= 1000 {
            return Err(invalid("moving_overlap_permille", "must be below 1000"));
        }
        if self.side_horizon_factor == 0 {
            return Err(invalid("side_horizon_factor", "must be at least 1"));
        }
        if self.crowd_iterations < u16::from(self.iterations) {
            return Err(invalid("crowd_iterations", "must be at least iterations"));
        }
        if self.crowd_iterations > MAX_CROWD_ITERATIONS {
            return Err(invalid("crowd_iterations", "must be at most 1000"));
        }
        if self.crowd_depth == 0 {
            return Err(invalid("crowd_depth", "must be at least 1"));
        }
        Ok(self)
    }
}

/// Movement rules loaded from `movement_rules.ron`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MovementRules {
    lane_tolerance_permille: u16,
    acquire_leash_radius: u16,
    contact: ContactRules,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MovementRulesFile {
    group_routing: GroupRoutingFile,
    pursuit: PursuitFile,
    contact: ContactRules,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PursuitFile {
    acquire_leash_radius: u32,
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
        acquire_leash_radius: STARCRAFT_ACQUIRE_LEASH_RADIUS,
        contact: ContactRules::LEGACY,
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
            acquire_leash_radius: STARCRAFT_ACQUIRE_LEASH_RADIUS,
            contact: ContactRules::LEGACY,
        })
    }

    /// These rules with the leash of an automatic chase set to `radius` world units.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] for a radius of 0 or above `u16::MAX`.
    pub fn with_acquire_leash_radius(self, radius: u32) -> SimResult<Self> {
        let acquire_leash_radius = u16::try_from(radius)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| SimCoreError::CatalogInvalidValue {
                definition: "pursuit".to_owned(),
                field: "acquire_leash_radius".to_owned(),
                reason: "must be a whole number of world units from 1 to 65535",
            })?;
        Ok(Self {
            acquire_leash_radius,
            ..self
        })
    }

    /// These rules with the contact rules `contact`, validated.
    ///
    /// # Errors
    /// [`SimCoreError::CatalogInvalidValue`] when `contact` is out of range
    /// ([`ContactRules::validated`]).
    pub fn with_contact(self, contact: ContactRules) -> SimResult<Self> {
        Ok(Self {
            contact: contact.validated()?,
            ..self
        })
    }

    /// Parses and validates the text of `movement_rules.ron`.
    ///
    /// # Errors
    /// [`SimCoreError::RonDeserializationError`] for malformed RON or unknown or missing fields,
    /// and [`SimCoreError::CatalogInvalidValue`] for a tolerance above 1000 permille or contact
    /// rules out of range.
    pub fn from_ron_str(ron_data: &str) -> SimResult<Self> {
        let file: MovementRulesFile = ron::from_str(ron_data)
            .map_err(|e| SimCoreError::RonDeserializationError(e.to_string()))?;
        Self::with_lane_tolerance(file.group_routing.lane_tolerance_permille)?
            .with_acquire_leash_radius(file.pursuit.acquire_leash_radius)?
            .with_contact(file.contact)
    }

    /// The contact rules (plan `2026-11-20-contact-solver` §3.6).
    #[must_use]
    pub const fn contact(self) -> ContactRules {
        self.contact
    }

    /// The leash of an automatically acquired chase, world units: the chase is given up when
    /// the pursuer stands farther than this from where it began (B-142).
    #[must_use]
    pub fn acquire_leash_radius(self) -> SimScalar {
        SimScalar::from_i32(i32::from(self.acquire_leash_radius))
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

    /// A `movement_rules.ron` text with the given leash and contact section.
    fn text(leash: u32, contact: &str) -> String {
        format!(
            "(group_routing: (lane_tolerance_permille: 75), pursuit: (acquire_leash_radius: \
             {leash}), contact: {contact})"
        )
    }

    /// The shipped contact section's values, as RON.
    const CONTACT: &str = "(pipeline: Legacy, iterations: 21, dead_band_permille: 10, \
        moving_overlap_permille: 50, side_horizon_factor: 2, moving_friends_push: true)";

    #[test]
    fn the_leash_is_read_and_must_be_positive() -> SimResult<()> {
        let rules = MovementRules::from_ron_str(&text(21, CONTACT))?;
        assert_eq!(rules.acquire_leash_radius(), SimScalar::from_i32(21));
        assert_eq!(rules.lane_tolerance_permille(), 75);
        assert!(matches!(
            MovementRules::from_ron_str(&text(0, CONTACT)),
            Err(SimCoreError::CatalogInvalidValue { .. })
        ));
        Ok(())
    }

    /// Plan `2026-11-20-contact-solver` §3.6: the contact section is read whole and validated:
    /// `iterations` ≥ 1, `0 < dead_band_permille < moving_overlap_permille < 1000` and
    /// `side_horizon_factor` ≥ 1; the pipeline is `Legacy` or `Solver`.
    #[test]
    fn the_contact_section_is_read_and_validated() -> SimResult<()> {
        let rules = MovementRules::from_ron_str(&text(21, CONTACT))?;
        assert_eq!(rules.contact(), ContactRules::LEGACY);
        let solver = CONTACT.replace("Legacy", "Solver").replace("true", "false");
        let rules = MovementRules::from_ron_str(&text(21, &solver))?;
        assert_eq!(rules.contact().pipeline, MovementPipeline::Solver);
        assert!(!rules.contact().moving_friends_push);
        for (from, to) in [
            ("iterations: 21", "iterations: 0"),
            ("dead_band_permille: 10", "dead_band_permille: 0"),
            ("dead_band_permille: 10", "dead_band_permille: 50"),
            (
                "moving_overlap_permille: 50",
                "moving_overlap_permille: 1000",
            ),
            ("side_horizon_factor: 2", "side_horizon_factor: 0"),
        ] {
            assert!(
                matches!(
                    MovementRules::from_ron_str(&text(21, &CONTACT.replace(from, to))),
                    Err(SimCoreError::CatalogInvalidValue { .. })
                ),
                "{to}"
            );
        }
        for broken in [
            CONTACT.replace("Legacy", "Other"),
            CONTACT.replace(", moving_friends_push: true", ""),
            CONTACT.replace("true)", "true, extra: 1)"),
        ] {
            assert!(
                matches!(
                    MovementRules::from_ron_str(&text(21, &broken)),
                    Err(SimCoreError::RonDeserializationError(_))
                ),
                "{broken}"
            );
        }
        Ok(())
    }

    #[test]
    fn malformed_text_and_unknown_or_missing_fields_are_rejected() {
        for text in [
            "(",
            "(group_routing: (lane_tolerance_permille: -1))",
            "(group_routing: (lane_tolerance_permille: 75, extra: 1))",
            "(group_routing: ())",
            "(group_routing: (lane_tolerance_permille: 75))",
            "(group_routing: (lane_tolerance_permille: 75), pursuit: ())",
            "(group_routing: (lane_tolerance_permille: 75), pursuit: (acquire_leash_radius: 21))",
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
    /// P-4 (plan `2026-11-26-friend-crowd-overlap` WP1): `crowd_iterations` absent reads
    /// `iterations` (the extension off) and `crowd_depth` absent reads the reference depth; a
    /// value below `iterations`, above 1,000 or a zero depth is refused with a typed error naming
    /// the field.
    #[test]
    fn the_crowd_fields_default_off_and_are_validated() -> SimResult<()> {
        let absent = MovementRules::from_ron_str(&text(21, CONTACT))?.contact();
        assert_eq!(absent.crowd_iterations, 21);
        assert!(!absent.crowd_extension_on());
        assert_eq!(absent.crowd_depth, REFERENCE_CROWD_DEPTH);
        let with = |fields: &str| CONTACT.replace("true)", &format!("true, {fields})"));
        let on =
            MovementRules::from_ron_str(&text(21, &with("crowd_iterations: 120, crowd_depth: 9")))?
                .contact();
        assert_eq!((on.crowd_iterations, on.crowd_depth), (120, 9));
        assert!(on.crowd_extension_on());
        let most = MovementRules::from_ron_str(&text(21, &with("crowd_iterations: 1000")))?;
        assert_eq!(most.contact().crowd_iterations, MAX_CROWD_ITERATIONS);
        for (fields, field) in [
            ("crowd_iterations: 20", "crowd_iterations"),
            ("crowd_iterations: 1001", "crowd_iterations"),
            ("crowd_depth: 0", "crowd_depth"),
        ] {
            let result = MovementRules::from_ron_str(&text(21, &with(fields)));
            assert!(
                matches!(
                    &result,
                    Err(SimCoreError::CatalogInvalidValue { field: named, .. }) if named == field
                ),
                "{fields}: {result:?}"
            );
        }
        Ok(())
    }

    /// Deviation D-δ (Q0095): the nudge switch is on when the file omits it, and the file can
    /// switch it off.
    #[test]
    fn the_nudge_switch_defaults_on_and_can_be_switched_off() -> SimResult<()> {
        let absent = MovementRules::from_ron_str(&text(21, CONTACT))?;
        assert!(absent.contact().nudge_engaged_along_ring);
        let off = CONTACT.replace("true)", "true, nudge_engaged_along_ring: false)");
        let rules = MovementRules::from_ron_str(&text(21, &off))?;
        assert!(!rules.contact().nudge_engaged_along_ring);
        Ok(())
    }
}
