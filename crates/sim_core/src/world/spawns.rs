//! Helpers shared by every unit-creating mechanism: where a created unit may stand and how long a
//! timed life lasts (plan `2026-12-01-death-spawns-and-hangar-commands` D-4).
//!
//! # Responsibility
//! Owns the placement search a created unit is put through (the effect engine's ring search over
//! terrain, structures and the map edge) and the draw of a created unit's timed life from the
//! world's seeded generator. Both were written for the broodlings plan's impact spawns (its D-4,
//! D-5) and moved here unchanged when death spawns needed them.
//!
//! # How it fits
//! [`crate::combat_resolution`]'s impact spawns and [`super::death_spawns`] call both while they
//! create units through the production spawn transaction. Placement only reads the movement
//! module's motion policy and footprint test; nothing here edits movement state.
//!
//! # Invariants
//! - Placement reads terrain, structures and the map edge only, never other units (the effect
//!   engine's placement rule, plan `2026-11-10` D-8; B-236): overlaps are separated by the next
//!   tick's movement.
//! - The rings are tried in a fixed order (the centre, then 0.5, 1.0, … world units, eight
//!   directions each, counter-clockwise from +x), so the chosen point is deterministic.
//! - A timed life draws exactly one integer from the world's generator when its random extra is
//!   positive, and none otherwise, so the generator's position is a function of the spawns made.

use crate::entity::Entity;
use crate::movement::geometry::ground_footprint_connected;
use crate::movement::{MotionPolicy, MovementLayer};
use crate::world::SimWorld;
use rand::Rng;
use sim_types::{SimAngle, SimScalar, SimVec2, TimedLifeSpec};

/// Step between the rings of the placement search, world units (the effect engine's ring step).
const RING_STEP: SimScalar = SimScalar::HALF;

/// Directions searched on each ring, counter-clockwise from +x.
const RING_DIRECTIONS: i32 = 8;

impl SimWorld {
    /// The first point valid for the not yet committed `unit`'s movement layer among `centre`,
    /// then the eight directions (counter-clockwise from +x) at 0.5, 1.0, … up to `range` world
    /// units: the rings of the effect engine's placement search. Only terrain, structures and the
    /// map edge count, as for a displacement's landing; other units never block the point.
    /// `None` when no point within `range` is valid (a `range` of 0 tries `centre` alone).
    pub(crate) fn spawn_place(
        &self,
        unit: &Entity,
        centre: SimVec2,
        range: SimScalar,
    ) -> Option<SimVec2> {
        let policy = MotionPolicy::resolve(&self.catalog, &self.map, unit).ok()?;
        let valid = |point: SimVec2| {
            policy.contains(&self.map, point)
                && (policy.layer != MovementLayer::Ground
                    || ground_footprint_connected(
                        &self.map,
                        point,
                        point,
                        policy.terrain_radius(),
                        None,
                    ))
        };
        if valid(centre) {
            return Some(centre);
        }
        let mut distance = RING_STEP;
        while distance <= range {
            for step in 0..RING_DIRECTIONS {
                let angle = SimAngle::from_degrees(360 / RING_DIRECTIONS * step);
                let point = centre + SimVec2::new(angle.cos(), angle.sin()) * distance;
                if valid(point) {
                    return Some(point);
                }
            }
            distance = distance.saturating_add(RING_STEP);
        }
        None
    }

    /// The ticks of life a created unit gets from `life` (plan `2026-11-28-broodlings` D-5):
    /// `ticks` plus a uniform draw of 0 to `random_extra_ticks` inclusive from the world's
    /// generator (no draw when it is 0). `None` for a unit created without a timed life.
    pub(crate) fn draw_timed_life(&mut self, life: Option<TimedLifeSpec>) -> Option<u16> {
        life.map(|life| {
            let extra = if life.random_extra_ticks == 0 {
                0
            } else {
                self.rng.gen_range::<u16, _>(0..=life.random_extra_ticks)
            };
            life.ticks.saturating_add(extra)
        })
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    //! Plan `2026-11-28-broodlings` D-4, D-5 (moved by plan
    //! `2026-12-01-death-spawns-and-hangar-commands` D-4): the placement search and the clock draw.
    use crate::test_support::world;
    use sim_map::{CliffLevel, GameMap, MapCell};
    use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, TimedLifeSpec, UnitTypeId};

    /// The first shipped ground unit and a structure type, by id.
    fn ground_and_structure(world: &crate::SimWorld) -> (UnitTypeId, UnitTypeId) {
        let mut ground: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| a.mover_type == MoverType::Ground && a.max_speed > SimScalar::ZERO)
            .map(|a| a.unit_type_id)
            .collect();
        ground.sort();
        let mut domes: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| matches!(a.shape, sim_types::ShapeKind::BuildingDome { .. }))
            .map(|a| a.unit_type_id)
            .collect();
        domes.sort();
        (ground[0], domes[0])
    }

    /// An open point is its own place; a point inside a structure moves to the first ring point
    /// clear of it, within the range; a range too short finds none.
    #[test]
    fn the_search_takes_the_centre_or_the_first_clear_ring_point() {
        let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0))).expect("map");
        let mut world = world(map, 1);
        let (unit, dome) = ground_and_structure(&world);
        let structure = world
            .spawn_structure(dome, PlayerId(1), 20, 20)
            .expect("a structure");
        let centre = world.entities[&structure].position;
        let probe = world
            .prepare_spawn(unit, PlayerId(0), centre, 0)
            .expect("a spawn")
            .entity;
        let open = SimVec2::from_i32(5, 5);
        assert_eq!(world.spawn_place(&probe, open, SimScalar::ZERO), Some(open));
        let place = world
            .spawn_place(&probe, centre, SimScalar::from_i32(5))
            .expect("a place within 5");
        assert!(place.distance(centre) <= SimScalar::from_i32(5));
        assert_ne!(place, centre);
        assert_eq!(world.spawn_place(&probe, centre, SimScalar::ZERO), None);
    }

    /// No timed life draws nothing; a fixed one draws nothing and gives its ticks; a random
    /// extra draws once per call, within `ticks ..= ticks + extra`, the same for the same seed.
    #[test]
    fn the_clock_draws_once_per_unit_only_when_it_varies() {
        let map = || GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0))).expect("map");
        let mut first = world(map(), 9);
        let start = first.rng.get_word_pos();
        assert_eq!(first.draw_timed_life(None), None);
        let fixed = TimedLifeSpec {
            ticks: 143,
            random_extra_ticks: 0,
        };
        assert_eq!(first.draw_timed_life(Some(fixed)), Some(143));
        assert_eq!(first.rng.get_word_pos(), start, "no draw without an extra");
        let varying = TimedLifeSpec {
            ticks: 143,
            random_extra_ticks: 5,
        };
        let drawn: Vec<_> = (0..64)
            .map(|_| first.draw_timed_life(Some(varying)).expect("a clock"))
            .collect();
        assert!(drawn.iter().all(|ticks| (143..=148).contains(ticks)));
        assert_ne!(first.rng.get_word_pos(), start);
        let mut second = world(map(), 9);
        let again: Vec<_> = (0..64)
            .map(|_| second.draw_timed_life(Some(varying)).expect("a clock"))
            .collect();
        assert_eq!(drawn, again);
    }
}
