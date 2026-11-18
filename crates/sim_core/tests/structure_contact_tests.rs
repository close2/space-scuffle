//! # Integration Test: structures meet units by their footprint (B-131)
//!
//! # Responsibility
//! Plan `2026-11-18-structure-contact`: a registered structure is not a body. It meets a ground
//! unit only through its footprint, at the unit's terrain radius `t`, never through a circle of
//! half its footprint width at the unit's collision radius `r`:
//! - S-9 (property): no teleport or unload point is refused as occupied by a structure; the
//!   footprint refuses it exactly when the point's square envelope at `t` meets it.
//!
//! S-2 (placement at a face middle) is `terrain_radius_tests`' T-6; S-5 (production exits) is a
//! unit test in `production/exits.rs`.
//!
//! # How it fits
//! Plays `SimWorld` on small synthetic maps with the shipped catalogs. The bodies under test are
//! scratch archetypes cloned from a shipped mobile unit and given their radii here, as in
//! `terrain_radius_tests`; structures are chosen by shape and footprint size, never by name
//! (charter §3.3).
//!
//! # Invariants
//! - A structure is found by `ShapeKind::BuildingDome` and its footprint size only.

#![allow(clippy::expect_used)]

use proptest::prelude::*;
use sim_core::movement::DisplacementAdmissionError;
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{MoverType, PlayerId, ShapeKind, SimScalar, SimVec2, UnitTypeId};

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

/// A flat level-1 map of `size` × `size`.
fn flat(size: u32) -> GameMap {
    GameMap::new(size, size, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a valid size")
}

/// A world on `map` with the shipped catalogs.
fn world_on(map: GameMap) -> SimWorld {
    sim_fixtures::world(map, 0x0B_131)
}

/// Registers a clone of the first shipped mobile ground unit under a free id: unarmed, with
/// collision radius `collision` and terrain radius `terrain` (world units), speed 0.2 per tick
/// reached at once, no turning delay, no capability; `edit` changes it further.
fn register(
    world: &mut SimWorld,
    slug: &str,
    collision: f64,
    terrain: f64,
    edit: impl FnOnce(&mut UnitArchetype),
) -> UnitTypeId {
    let base = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.mover_type == MoverType::Ground
                && arch.max_speed > SimScalar::ZERO
                && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
        })
        .expect("invariant: the shipped catalog has a mobile ground unit")
        .clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("invariant: a free unit type id");
    let mut arch = base;
    arch.unit_type_id = free;
    slug.clone_into(&mut arch.slug);
    slug.clone_into(&mut arch.name);
    arch.collision_radius = s(collision);
    arch.terrain_radius = s(terrain);
    arch.max_speed = s(0.2);
    arch.acceleration = s(10.0);
    arch.deceleration = s(10.0);
    arch.turn_rate = sim_types::SimAngle::from_degrees(180);
    arch.stationary_turn_rate = sim_types::SimAngle::from_degrees(180);
    arch.turn_before_move = false;
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.capabilities.overclock = None;
    arch.capabilities.barrier = None;
    arch.capabilities.attack_dash = None;
    edit(&mut arch);
    world.catalog.register(arch);
    free
}

/// The shipped structure archetypes with a square footprint of `size` cells, in catalog order.
fn structures_of_size(world: &SimWorld, size: u8) -> Vec<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .filter(|a| {
            matches!(
                a.shape,
                ShapeKind::BuildingDome { footprint_w, footprint_h }
                    if footprint_w == size && footprint_h == size
            )
        })
        .map(|a| a.unit_type_id)
        .collect()
}

/// The collision radii S-9 draws from, world units.
const RADII: [f64; 4] = [0.375, 0.5, 0.625, 0.875];
/// How far below the collision radius S-9 draws the terrain radius, world units.
const TERRAIN_DROPS: [f64; 4] = [0.0, 0.0625, 0.125, 0.25];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// S-9: beside a seeded shipped structure of footprint 2, 3 or 5, a scratch body of
    /// collision radius `r` and terrain radius `t ≤ r` is placed at 200 seeded points within
    /// `h + r + 1` of the structure's centre (`h` its half-width). No point is refused as
    /// `Occupied` by the structure, and a point is refused `Terrain` exactly when the open square
    /// of half-side `t` around it meets the footprint's closed box, which is the envelope
    /// `ground_footprint_connected` tests: the point is refused when it lies strictly inside the
    /// footprint widened by `t` on every side (`segment_overlaps_open_box`), so a point exactly
    /// `t` from a face is admitted. Every other point is admitted.
    #[test]
    fn no_placement_is_refused_as_occupied_by_a_structure(
        size_index in 0usize..3,
        pick in 0usize..64,
        radius_index in 0usize..4,
        drop_index in 0usize..4,
        offsets in prop::collection::vec((-1.0f64..1.0, -1.0f64..1.0), 200),
    ) {
        let r = RADII[radius_index];
        let t = r - TERRAIN_DROPS[drop_index];
        prop_assume!(t > 0.0);
        let mut world = world_on(flat(40));
        let size = [2_u8, 3, 5][size_index];
        let kinds = structures_of_size(&world, size);
        prop_assert!(!kinds.is_empty(), "the shipped catalog has a {size}x{size} structure");
        let structure = kinds[pick % kinds.len()];
        let placed = world
            .spawn_structure(structure, PlayerId(1), 18, 18)
            .expect("open ground");
        let kind = register(&mut world, "structure_contact_scratch", r, t, |_| {});
        let body = world
            .spawn_unit(kind, PlayerId(0), at(3.0, 3.0))
            .expect("spawn");
        let footprint = &world.map.structures()[&placed];
        let (left, bottom) = (
            i128::from(footprint.top_left.0) << 32,
            i128::from(footprint.top_left.1) << 32,
        );
        let (right, top) = (
            left + (i128::from(footprint.width) << 32),
            bottom + (i128::from(footprint.height) << 32),
        );
        let half = f64::from(footprint.width) / 2.0;
        let centre = (
            f64::from(footprint.top_left.0) + half,
            f64::from(footprint.top_left.1) + f64::from(footprint.height) / 2.0,
        );
        let reach = half + r + 1.0;
        let t_raw = i128::from(s(t).raw());
        for (dx, dy) in offsets {
            let point = at(centre.0 + dx * reach, centre.1 + dy * reach);
            let (px, py) = (i128::from(point.x.raw()), i128::from(point.y.raw()));
            let inside = left - t_raw < px
                && px < right + t_raw
                && bottom - t_raw < py
                && py < top + t_raw;
            let result = world.validate_displacement_destination(body, point);
            prop_assert!(
                !matches!(result, Err(DisplacementAdmissionError::Occupied(id)) if id == placed),
                "({}, {}) refused as occupied by the structure (r {r}, t {t})",
                point.x.to_f32(),
                point.y.to_f32()
            );
            if inside {
                prop_assert!(
                    matches!(result, Err(DisplacementAdmissionError::Terrain(id)) if id == body),
                    "({}, {}) inside the footprint's t-envelope: {result:?}",
                    point.x.to_f32(),
                    point.y.to_f32()
                );
            } else {
                prop_assert!(
                    result.is_ok(),
                    "({}, {}) outside the footprint's t-envelope (r {r}, t {t}): {result:?}",
                    point.x.to_f32(),
                    point.y.to_f32()
                );
            }
        }
    }
}
