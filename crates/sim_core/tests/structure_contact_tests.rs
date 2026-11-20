//! # Integration Test: structures meet units by their footprint (B-131)
//!
//! # Responsibility
//! Plan `2026-11-18-structure-contact`: a registered structure is not a body. It meets a ground
//! unit only through its footprint, at the unit's terrain radius `t`, never through a circle of
//! half its footprint width at the unit's collision radius `r`:
//! - S-1: a unit pushed into the middle of a structure face rests at `t`, not `r`;
//! - S-3: a structure neither moves nor pushes a body outside its footprint's `t`-envelope;
//! - S-4: a unit walks straight past a face inside the structure's old circle;
//! - S-6, S-7 (guards): units still separate at their radii, and an immobile *unit* is still a
//!   fixed circle body;
//! - S-8: a body wedged nearer than `t` is not thrown out, and walks away never deeper;
//! - S-9 (property): no teleport or unload point is refused as occupied by a structure; the
//!   structure refuses it exactly when the disc of radius `t` meets its footprint shape (plan
//!   `2026-11-18-contour-footprints` D-8; before, the point's square envelope at `t` and the
//!   footprint's cells);
//! - S-10 (property): the collision phase never corrects a lone body outside a structure's
//!   footprint `t`-envelope.
//!
//! S-2 (placement at a face middle) is `terrain_radius_tests`' T-6; S-5 (production exits) is a
//! unit test in `production/exits.rs`; the collision body set and the steering obstacles are pinned
//! by unit tests in `movement/collision.rs` and `movement/locomotion.rs`.
//!
//! # How it fits
//! Plays `SimWorld` on small synthetic maps with the shipped catalogs. The bodies under test are
//! scratch archetypes cloned from a shipped mobile unit and given their radii here, as in
//! `terrain_radius_tests`; structures are chosen by shape and footprint size, never by name
//! (charter §3.3).
//!
//! # Invariants
//! - A structure is found by `ShapeKind::BuildingDome` and its footprint size only.
//! - Every tick of `step` ends with no simulation diagnostic.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use proptest::prelude::*;
use sim_core::movement::DisplacementAdmissionError;
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};

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
                ShapeKind::BuildingDome { footprint_w, footprint_h, .. }
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

/// The footprint shapes S-9 draws from, by footprint size: the full box and the 5.0.14
/// contours of that size (plan `2026-11-18-contour-footprints` §2.3), in quarter cells.
fn shapes_of_size(size: u8) -> Vec<sim_types::FootprintShape> {
    let shape = |half: u16, chamfer: u16| sim_types::FootprintShape::Box {
        half_width_q: half,
        half_height_q: half,
        chamfer_q: chamfer,
    };
    match size {
        2 => vec![shape(4, 0), shape(4, 2), shape(2, 1)],
        3 => vec![shape(6, 0), shape(6, 2), shape(6, 3)],
        _ => vec![shape(10, 0), shape(10, 6)],
    }
}

/// A point on the Q16 grid (raw coordinates rounded down to a multiple of 2⁻¹⁶).
fn on_q16(point: SimVec2) -> SimVec2 {
    let snap = |v: SimScalar| SimScalar::from_raw(v.raw() & !0xFFFF);
    SimVec2::new(snap(point.x), snap(point.y))
}

/// Whether the Q16 point `p` lies closer than `t` (a multiple of 2⁻¹⁶) to the closed shape of
/// `placed`, or inside it: an exact `i128` oracle written apart from `sim_map`'s predicates.
fn disc_meets_shape(placed: &sim_map::PlacedStructure, p: SimVec2, t: SimScalar) -> bool {
    let footprint = placed.footprint();
    let vertices = footprint.contour();
    let count = vertices.len();
    // Lattice units are quarter cells: 2^14 Q16 units.
    let q = |v: sim_map::LatticePoint| (i128::from(v.x) << 14, i128::from(v.y) << 14);
    let (px, py) = (i128::from(p.x.raw() >> 16), i128::from(p.y.raw() >> 16));
    let t_q = i128::from(t.raw() >> 16);
    let mut inside = true;
    let mut near = false;
    for i in 0..count {
        let ((ux, uy), (vx, vy)) = (q(vertices[i]), q(vertices[(i + 1) % count]));
        let (dx, dy) = (vx - ux, vy - uy);
        let (wx, wy) = (px - ux, py - uy);
        // Counter-clockwise contour: the interior is on the left of every edge.
        if dx * wy - dy * wx < 0 {
            inside = false;
        }
        let length_sq = dx * dx + dy * dy;
        let along = wx * dx + wy * dy;
        let (num, den) = if along <= 0 {
            (wx * wx + wy * wy, 1)
        } else if along >= length_sq {
            let (ex, ey) = (px - vx, py - vy);
            (ex * ex + ey * ey, 1)
        } else {
            let cross = dx * wy - dy * wx;
            (cross * cross, length_sq)
        };
        if num < t_q * t_q * den {
            near = true;
        }
    }
    inside || near
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// S-9, rewritten by plan `2026-11-18-contour-footprints` WP5 (D-8): beside a seeded scratch
    /// structure of footprint 2, 3 or 5 with a full or a contour shape, a scratch body of
    /// collision radius `r` and terrain radius `t ≤ r` is placed at 200 seeded points on the Q16
    /// grid within `h + r + 1` of the structure's centre. No point is refused as `Occupied` by
    /// the structure, and a point is refused `Terrain` exactly when the disc of radius `t` meets
    /// the closed shape (an exact oracle: the point is inside the shape, or closer than `t` to
    /// it); every other point is admitted. At a face the disc and the old square envelope
    /// agree; they differ within `t(√2 − 1)` of a convex corner.
    #[test]
    fn no_placement_is_refused_except_by_the_disc_against_the_shape(
        size_index in 0usize..3,
        pick in 0usize..64,
        shape_index in 0usize..3,
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
        let shapes = shapes_of_size(size);
        let shape = shapes[shape_index % shapes.len()];
        let mut scratch = world
            .catalog
            .get(kinds[pick % kinds.len()])
            .expect("invariant: chosen from the catalog")
            .clone();
        let free = (1..=u16::MAX)
            .map(UnitTypeId)
            .find(|id| world.catalog.get(*id).is_err())
            .expect("invariant: a free unit type id");
        scratch.unit_type_id = free;
        "s9_scratch_structure".clone_into(&mut scratch.slug);
        scratch.shape = ShapeKind::BuildingDome { footprint_w: size, footprint_h: size, shape };
        world.catalog.register(scratch);
        let placed = world
            .spawn_structure(free, PlayerId(1), 18, 18)
            .expect("open ground");
        let kind = register(&mut world, "structure_contact_scratch", r, t, |_| {});
        let body = world
            .spawn_unit(kind, PlayerId(0), at(3.0, 3.0))
            .expect("spawn");
        let footprint = world.map.structures()[&placed].clone();
        let half = f64::from(footprint.width) / 2.0;
        let centre = (
            f64::from(footprint.top_left.0) + half,
            f64::from(footprint.top_left.1) + f64::from(footprint.height) / 2.0,
        );
        let reach = half + r + 1.0;
        for (dx, dy) in offsets {
            let point = on_q16(at(centre.0 + dx * reach, centre.1 + dy * reach));
            let meets = disc_meets_shape(&footprint, point, s(t));
            let result = world.validate_displacement_destination(body, point);
            prop_assert!(
                !matches!(result, Err(DisplacementAdmissionError::Occupied(id)) if id == placed),
                "({}, {}) refused as occupied by the structure (r {r}, t {t})",
                point.x.to_f32(),
                point.y.to_f32()
            );
            if meets {
                prop_assert!(
                    matches!(result, Err(DisplacementAdmissionError::Terrain(id)) if id == body),
                    "({}, {}) meets the shape {shape:?} at t {t}: {result:?}",
                    point.x.to_f32(),
                    point.y.to_f32()
                );
            } else {
                prop_assert!(
                    result.is_ok(),
                    "({}, {}) clear of the shape {shape:?} (r {r}, t {t}): {result:?}",
                    point.x.to_f32(),
                    point.y.to_f32()
                );
            }
        }
    }
}

/// The terrain radius of the slim body, world units.
const T: f64 = 0.3125;
/// The collision radius of the slim body, world units.
const R: f64 = 0.5;
/// The contact rule's rounding allowance (`WALL_CONTACT_TOLERANCE`, 2⁻²⁰ world units).
const TOLERANCE: f64 = 1.0 / 1_048_576.0;
/// The minimum cell of the 3×3 structure of S-1 to S-8: it covers x and y in [18, 21], so its
/// west face is x = 18 with its middle at y = 19.5, and its south face is y = 18.
const CELL: u32 = 18;

fn xy(point: SimVec2) -> (f64, f64) {
    (f64::from(point.x.to_f32()), f64::from(point.y.to_f32()))
}

/// The slim body: collision radius [`R`], terrain radius [`T`].
fn slim(world: &mut SimWorld) -> UnitTypeId {
    register(world, "structure_contact_slim", R, T, |_| {})
}

fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands,
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn step(world: &mut SimWorld) {
    step_with(world, Vec::new());
}

fn move_to(owner: u8, id: EntityId, target: SimVec2) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(owner),
        entities: vec![id],
        order: UnitOrder::Move { target },
        queue: false,
    }
}

fn hold(world: &mut SimWorld, id: EntityId) {
    world
        .entities
        .get_mut(&id)
        .expect("spawned")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
}

/// The first shipped unarmed structure archetype with a square footprint of `size` cells.
fn unarmed_structure(world: &SimWorld, size: u8) -> UnitTypeId {
    structures_of_size(world, size)
        .into_iter()
        .find(|kind| {
            world.catalog.get(*kind).is_ok_and(|a| {
                a.weapon.is_none() && a.ground_weapon.is_none() && a.air_weapon.is_none()
            })
        })
        .expect("invariant: the shipped catalog has an unarmed structure of this size")
}

/// A world on a flat 40 × 40 map with an unarmed 3×3 structure of player 0 at [`CELL`].
fn with_structure() -> (SimWorld, EntityId) {
    let mut world = world_on(flat(40));
    let kind = unarmed_structure(&world, 3);
    let placed = world
        .spawn_structure(kind, PlayerId(0), CELL, CELL)
        .expect("open ground");
    (world, placed)
}

/// The Euclidean distance from `point` to the footprint of `structure`, world units.
fn gap_to_footprint(world: &SimWorld, structure: EntityId, point: SimVec2) -> f64 {
    let placed = &world.map.structures()[&structure];
    let (left, bottom) = (f64::from(placed.top_left.0), f64::from(placed.top_left.1));
    let (right, top) = (
        left + f64::from(placed.width),
        bottom + f64::from(placed.height),
    );
    let (x, y) = xy(point);
    let dx = (left - x).max(0.0).max(x - right);
    let dy = (bottom - y).max(0.0).max(y - top);
    dx.hypot(dy)
}

/// S-1: a slim body walks to 0.05 beyond its terrain radius from the middle of a 3×3
/// structure's west face, then a hostile slim body holding beside it pushes it at 45° into the
/// face, as `terrain_radius_tests` T-1 does against a cliff. On every tick it stays at least its
/// terrain radius from the footprint, and it comes to rest at its terrain radius: the structure
/// is no circle that holds it at its collision radius.
#[test]
fn a_unit_rests_at_its_terrain_radius_at_the_middle_of_a_structure_face() {
    let (mut world, structure) = with_structure();
    let kind = slim(&mut world);
    let face = f64::from(CELL);
    let middle = face + 1.5;
    let body = world
        .spawn_unit(kind, PlayerId(0), at(12.0, middle))
        .expect("spawn");
    let gap = |world: &SimWorld| gap_to_footprint(world, structure, world.entities[&body].position);
    step_with(
        &mut world,
        vec![move_to(0, body, at(face - T - 0.05, middle))],
    );
    for _ in 0..60 {
        step(&mut world);
        assert!(gap(&world) >= T - TOLERANCE, "{}", gap(&world));
    }
    let walked = xy(world.entities[&body].position);
    let pusher = world
        .spawn_unit(kind, PlayerId(1), at(walked.0 - 0.45, walked.1 + 0.45))
        .expect("spawn");
    hold(&mut world, pusher);
    for _ in 0..20 {
        step(&mut world);
        assert!(gap(&world) >= T - TOLERANCE, "{}", gap(&world));
    }
    let rest = gap(&world);
    let (_, y) = xy(world.entities[&body].position);
    assert!(
        y > face && y < face + 3.0,
        "still beside the west face: {y}"
    );
    assert!(
        (rest - T).abs() <= 1e-3,
        "rests at its terrain radius {T} from the face, not its radius {R}: {rest}"
    );
}

/// S-3: an idle slim body 0.4 west of the middle of a structure's west face (outside the
/// footprint widened by its terrain radius, inside the old circle at its collision radius) is
/// left exactly where it is for five ticks, and the structure neither moves nor changes its
/// footprint (plan 49's invariant: a foundation never moves).
#[test]
fn a_structure_neither_moves_nor_pushes_a_body_outside_its_footprint_envelope() {
    let (mut world, structure) = with_structure();
    let kind = slim(&mut world);
    let face = f64::from(CELL);
    let body = world
        .spawn_unit(kind, PlayerId(0), at(face - 0.4, face + 1.5))
        .expect("spawn");
    let start = world.entities[&body].position;
    let dome = world.entities[&structure].position;
    let placed = world.map.structures()[&structure].clone();
    for _ in 0..5 {
        step(&mut world);
        assert_eq!(world.entities[&body].position, start, "not pushed");
        assert_eq!(world.entities[&structure].position, dome, "the dome stays");
        assert_eq!(
            world.map.structures()[&structure],
            placed,
            "the footprint stays"
        );
    }
}

/// S-4: a slim body walks east along a structure's south face (y = 18) at 0.4 below it:
/// between its terrain radius and its collision radius, and clear of the funnel's corner offset
/// `T + 0.05`. Its route is the straight line, and nothing bends it: on every tick it keeps its
/// line, and it arrives.
#[test]
fn a_unit_walks_straight_past_a_structure_face_inside_its_old_circle() {
    let (mut world, _) = with_structure();
    let kind = slim(&mut world);
    let line = f64::from(CELL) - 0.4;
    let body = world
        .spawn_unit(kind, PlayerId(0), at(12.0, line))
        .expect("spawn");
    step_with(&mut world, vec![move_to(0, body, at(27.0, line))]);
    for _ in 0..120 {
        step(&mut world);
        let (x, y) = xy(world.entities[&body].position);
        assert!((y - line).abs() < 1e-3, "kept its line at x {x}: {y}");
    }
    let (x, y) = xy(world.entities[&body].position);
    assert!((x - 27.0).hypot(y - line) < 0.05, "arrived: ({x}, {y})");
}

/// S-6 (guard): two hostile slim bodies pressed together beside the middle of a structure's
/// west face, each 0.4 from the face, separate along the face to the sum of their collision
/// radii: units still meet units at `Radius`.
#[test]
fn two_units_beside_a_structure_still_separate_at_their_radii() {
    let (mut world, structure) = with_structure();
    let kind = slim(&mut world);
    let (x, middle) = (f64::from(CELL) - 0.4, f64::from(CELL) + 1.5);
    let first = world
        .spawn_unit(kind, PlayerId(0), at(x, middle - 0.3))
        .expect("spawn");
    let second = world
        .spawn_unit(kind, PlayerId(1), at(x, middle + 0.3))
        .expect("spawn");
    for _ in 0..20 {
        step(&mut world);
        for id in [first, second] {
            let gap = gap_to_footprint(&world, structure, world.entities[&id].position);
            assert!(gap >= T - TOLERANCE, "{gap}");
        }
    }
    let (a, b) = (
        xy(world.entities[&first].position),
        xy(world.entities[&second].position),
    );
    let distance = (a.0 - b.0).hypot(a.1 - b.1);
    assert!(
        distance >= 2.0 * R - 1e-3,
        "separated at their radii: {distance}"
    );
}

/// S-7 (guard): an immobile unit (a mobile archetype's entity with `is_immobile` set, as
/// `fixed_body_transactions_tests` does) is still a fixed circle body: it pushes an overlapping
/// hostile slim body out to the sum of their collision radii and does not move itself.
#[test]
fn an_immobile_unit_is_still_a_fixed_circle_body() {
    let mut world = world_on(flat(40));
    let kind = slim(&mut world);
    let anchor = world
        .spawn_unit(kind, PlayerId(0), at(10.0, 10.0))
        .expect("spawn");
    world
        .entities
        .get_mut(&anchor)
        .expect("spawned")
        .is_immobile = true;
    let body = world
        .spawn_unit(kind, PlayerId(1), at(10.6, 10.0))
        .expect("spawn");
    let fixed = world.entities[&anchor].position;
    for _ in 0..10 {
        step(&mut world);
        assert_eq!(
            world.entities[&anchor].position, fixed,
            "the immobile unit stays"
        );
    }
    let (x, y) = xy(world.entities[&body].position);
    let distance = (x - 10.0).hypot(y - 10.0);
    assert!(
        distance >= 2.0 * R - 1e-3,
        "pushed out to the sum of the radii: {distance}"
    );
}

/// S-8: a slim body standing 0.1 from the middle of a structure's west face, nearer than its
/// terrain radius (which no shipped path creates; D-8), is not thrown out: idle for five ticks it
/// stays exactly where it is. Ordered west, it walks away, and its distance from the footprint
/// never falls (the contact rule lets a wedged body move out, never deeper).
#[test]
fn a_body_wedged_against_a_footprint_is_not_thrown_out_and_walks_away() {
    let (mut world, structure) = with_structure();
    let kind = slim(&mut world);
    let (face, middle) = (f64::from(CELL), f64::from(CELL) + 1.5);
    let body = world
        .spawn_unit(kind, PlayerId(0), at(face - 0.1, middle))
        .expect("spawn");
    let start = world.entities[&body].position;
    for _ in 0..5 {
        step(&mut world);
        assert_eq!(world.entities[&body].position, start, "not thrown out");
    }
    let gap = |world: &SimWorld| gap_to_footprint(world, structure, world.entities[&body].position);
    let mut last = gap(&world);
    step_with(&mut world, vec![move_to(0, body, at(12.0, middle))]);
    for _ in 0..60 {
        step(&mut world);
        let now = gap(&world);
        assert!(now >= last - TOLERANCE, "never deeper: {last} then {now}");
        last = now;
    }
    assert!(last > T + 1.0, "walked away: {last}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// S-10: a seeded unarmed structure of footprint 2, 3 or 5 and a lone idle hostile slim body
    /// at a seeded point within `h + R + 0.5` of its centre, outside the footprint widened by `T`
    /// (the open envelope of S-9). One collision phase leaves the body exactly where it is: the
    /// structure is no body that corrects it.
    #[test]
    fn a_lone_body_beside_a_structure_is_never_corrected(
        size_index in 0usize..3,
        dx in -1.0f64..1.0,
        dy in -1.0f64..1.0,
    ) {
        let mut world = world_on(flat(40));
        let size = [2_u8, 3, 5][size_index];
        let structure = unarmed_structure(&world, size);
        let placed = world
            .spawn_structure(structure, PlayerId(0), CELL, CELL)
            .expect("open ground");
        let kind = slim(&mut world);
        let footprint = &world.map.structures()[&placed];
        let half = f64::from(footprint.width) / 2.0;
        let (left, bottom) = (f64::from(footprint.top_left.0), f64::from(footprint.top_left.1));
        let reach = half + R + 0.5;
        let point = at(left + half + dx * reach, bottom + half + dy * reach);
        let (px, py) = (i128::from(point.x.raw()), i128::from(point.y.raw()));
        let t_raw = i128::from(s(T).raw());
        let (l, b) = (
            i128::from(footprint.top_left.0) << 32,
            i128::from(footprint.top_left.1) << 32,
        );
        let (r, t) = (
            l + (i128::from(footprint.width) << 32),
            b + (i128::from(footprint.height) << 32),
        );
        prop_assume!(!(l - t_raw < px && px < r + t_raw && b - t_raw < py && py < t + t_raw));
        let body = world.spawn_unit(kind, PlayerId(1), point).expect("spawn");
        world.resolve_collisions_and_push();
        let diagnostics: Vec<_> = world.drain_diagnostics().collect();
        prop_assert!(diagnostics.is_empty(), "{diagnostics:?}");
        prop_assert_eq!(world.entities[&body].position, point);
    }
}

/// Plan `2026-11-18-contour-footprints` WP4: a structure is placed with its catalog footprint
/// shape. A scratch copy of a shipped 3×3 structure given a contour cut by 0.75 is spawned, and
/// the map's placed structure carries exactly that shape on the same cells; the shipped
/// structure places its own shape.
#[test]
fn a_spawned_structure_places_its_catalog_shape() {
    let mut world = world_on(flat(40));
    let shipped = unarmed_structure(&world, 3);
    let mut scratch = world
        .catalog
        .get(shipped)
        .expect("invariant: chosen from the catalog")
        .clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("invariant: a free unit type id");
    scratch.unit_type_id = free;
    "scratch_contour".clone_into(&mut scratch.slug);
    let contour = sim_types::FootprintShape::Box {
        half_width_q: 6,
        half_height_q: 6,
        chamfer_q: 3,
    };
    scratch.shape = ShapeKind::BuildingDome {
        footprint_w: 3,
        footprint_h: 3,
        shape: contour,
    };
    world.catalog.register(scratch);
    let placed = world
        .spawn_structure(free, PlayerId(0), 10, 10)
        .expect("open ground");
    let on_map = &world.map.structures()[&placed];
    assert_eq!(
        (on_map.top_left, on_map.width, on_map.height),
        ((10, 10), 3, 3)
    );
    assert_eq!(on_map.shape, contour);
    let own = world
        .spawn_structure(shipped, PlayerId(0), 20, 20)
        .expect("open ground");
    let shipped_shape = match world
        .catalog
        .get(shipped)
        .expect("invariant: chosen from the catalog")
        .shape
    {
        ShapeKind::BuildingDome { shape, .. } => Some(shape),
        ShapeKind::UnitSphere => None,
    };
    assert_eq!(Some(world.map.structures()[&own].shape), shipped_shape);
}
