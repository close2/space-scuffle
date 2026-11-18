//! # Integration Test: the terrain radius (B-116, `StarCraft` II's `InnerRadius`)
//!
//! # Responsibility
//! Plan `2026-11-18-inner-radius` WP2: a ground body meets terrain, structures and the map edge
//! at its terrain radius `t` (`UnitArchetype::terrain_radius`, the game's
//! `min(InnerRadius, Radius)`) and other bodies at its collision radius `r`:
//! - T-1: it rests against a wall at `t` and never goes below `t`, walking or pushed;
//! - T-2: it routes through a gap that fits `2t` and not `2r`;
//! - T-3: its corner waypoints keep `t` plus the corner margin from the corner;
//! - T-4: two such bodies still separate at their full radii;
//! - T-5: the map-edge domain admits the terrain disc; an air body's domain stays at `r`;
//! - T-6: a teleport or unload point beside a structure is judged at `t`, occupancy at `r`;
//! - T-7: a body at more than `t` from a construction site does not block it;
//! - T-9: the Charge path is tested at `t`;
//! - T-10: a group lane is searched at its leader's `t`.
//!
//! T-8 (production exits), T-11 (the contact property) and T-12 (the resolve invariant) are unit
//! tests in `production/exits.rs`, `movement/contact.rs` and `movement/mod.rs`; T-13 (routes are
//! monotone in the radius) is in `sim_map`'s `navmesh_tests.rs`.
//!
//! # How it fits
//! Plays `SimWorld::step` on small synthetic maps with the shipped catalogs. The bodies under test
//! are scratch archetypes cloned from a shipped mobile unit and given their radii here, so the
//! shipped data (WP3) cannot move these results; archetypes are chosen by capability, never by
//! name (charter §3.3).
//!
//! # Invariants
//! - Every tick ends with no simulation diagnostic.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use sim_core::movement::{DisplacementAdmissionError, MovementAdmissionError};
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    AttackDashSpec, EntityId, MoverType, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

/// The terrain radius of the slim body, world units.
const T: f64 = 0.3125;
/// The collision radius of the slim body, world units.
const R: f64 = 0.5;
/// The contact rule's rounding allowance (`WALL_CONTACT_TOLERANCE`, 2⁻²⁰ world units).
const TOLERANCE: f64 = 1.0 / 1_048_576.0;
/// The corner margin of the radius funnel (`sim_map::CORNER_SAFETY_MARGIN`, about 0.05).
const CORNER_MARGIN: f64 = 0.05;

fn s(value: f64) -> SimScalar {
    SimScalar::from_num(value)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(s(x), s(y))
}

fn xy(point: SimVec2) -> (f64, f64) {
    (f64::from(point.x.to_f32()), f64::from(point.y.to_f32()))
}

/// A flat level-1 map of `size` × `size` with the cells for which `wall` holds made cliffs.
fn walled(size: u32, wall: impl Fn(u32, u32) -> bool) -> GameMap {
    let mut map = GameMap::new(size, size, MapCell::default_ground(CliffLevel(1)))
        .expect("invariant: a valid size");
    for y in 0..size {
        for x in 0..size {
            if wall(x, y) {
                map.cell_mut(x, y).expect("inside").flags = CellFlags::UNPASSABLE_CLIFF;
            }
        }
    }
    map
}

/// A world on `map` with the shipped catalogs.
fn world_on(map: GameMap) -> SimWorld {
    sim_fixtures::world(map, 0x1B_116)
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

/// The slim body: collision radius [`R`], terrain radius [`T`].
fn slim(world: &mut SimWorld) -> UnitTypeId {
    register(world, "terrain_radius_slim", R, T, |_| {})
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

fn command(owner: u8, entities: Vec<EntityId>, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(owner),
        entities,
        order,
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

/// T-1: a body walks to a goal 0.05 beyond its terrain radius from a straight cliff face at x = 10
/// (a goal its collision disc would not fit, so it would be retargeted), then a hostile body
/// holding beside it pushes it at 45° into the face (as contact T-B2 does). It comes to rest at its
/// terrain radius from the face and is never nearer on any tick.
#[test]
fn a_body_rests_at_its_terrain_radius_against_a_wall() {
    let mut world = world_on(walled(20, |x, _| x >= 10));
    let kind = slim(&mut world);
    let body = world
        .spawn_unit(kind, PlayerId(0), at(7.0, 10.0))
        .expect("spawn");
    let gap = |world: &SimWorld| 10.0 - xy(world.entities[&body].position).0;
    let goal = 10.0 - T - 0.05;
    step_with(
        &mut world,
        vec![command(
            0,
            vec![body],
            UnitOrder::Move {
                target: at(goal, 10.0),
            },
        )],
    );
    for _ in 0..40 {
        step(&mut world);
        assert!(gap(&world) >= T - TOLERANCE, "{}", gap(&world));
    }
    let walked = xy(world.entities[&body].position);
    assert!(
        (walked.0 - goal).abs() < 0.01 && (walked.1 - 10.0).abs() < 0.01,
        "walked to its goal: {walked:?}"
    );
    let pusher = world
        .spawn_unit(kind, PlayerId(1), at(walked.0 - 0.45, walked.1 + 0.45))
        .expect("spawn");
    hold(&mut world, pusher);
    for _ in 0..20 {
        step(&mut world);
        assert!(gap(&world) >= T - TOLERANCE, "{}", gap(&world));
    }
    let rest = gap(&world);
    assert!(
        (rest - T).abs() <= 1e-3,
        "rests at its terrain radius {T} from the wall, not its radius {R}: {rest}"
    );
}

/// The shortest-way-round world of T-2 and T-10: a cliff column at x = 12 from y = 0 to 19,
/// open only at row 6 (one cell, 1.0 wide), and open above y = 20.
fn slot_map() -> GameMap {
    walled(24, |x, y| x == 12 && y < 20 && y != 6)
}

/// The body for the slot: collision radius 0.625 (`2r` = 1.25 does not fit the 1.0 slot),
/// terrain radius 0.375 (`2t` = 0.75 does) or 0.625.
fn slot_body(world: &mut SimWorld, terrain: f64) -> UnitTypeId {
    register(world, "terrain_radius_slot", 0.625, terrain, |_| {})
}

/// The highest y any waypoint of `id`'s route reaches.
fn route_top(world: &SimWorld, id: EntityId) -> f64 {
    world.entities[&id]
        .current_funnel_waypoints
        .iter()
        .map(|p| xy(*p).1)
        .fold(f64::MIN, f64::max)
}

/// T-2: a body whose terrain disc fits a one-cell slot routes through it and walks to the goal;
/// the same body without a smaller terrain radius routes round the wall's far end.
#[test]
fn a_gap_that_fits_the_terrain_disc_is_routed_through() {
    let goal = at(18.5, 6.5);
    for (terrain, through) in [(0.375, true), (0.625, false)] {
        let mut world = world_on(slot_map());
        let kind = slot_body(&mut world, terrain);
        let body = world
            .spawn_unit(kind, PlayerId(0), at(6.5, 6.5))
            .expect("spawn");
        step_with(
            &mut world,
            vec![command(0, vec![body], UnitOrder::Move { target: goal })],
        );
        let top = route_top(&world, body);
        if through {
            assert!(top < 8.0, "t = {terrain}: routed through the slot: {top}");
            let mut highest: f64 = 0.0;
            for _ in 0..150 {
                step(&mut world);
                highest = highest.max(xy(world.entities[&body].position).1);
            }
            let end = xy(world.entities[&body].position);
            assert!(highest < 8.0, "walked through the slot: {highest}");
            assert!((end.0 - 18.5).hypot(end.1 - 6.5) < 0.5, "arrived: {end:?}");
        } else {
            assert!(top > 19.0, "t = {terrain}: routed round the wall: {top}");
        }
    }
}

/// T-3: at the convex corner (16, 16) of a solid south-west quadrant, a body's corner waypoint
/// lies at its terrain radius plus the funnel's corner margin from the corner (the radius
/// funnel's offset, `sim_map::Pathfinder::find_path_with_vision`), nearer than its radius.
#[test]
fn corner_waypoints_keep_the_terrain_radius_and_margin() {
    let mut map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0))).expect("map");
    for y in 0..16 {
        for x in 0..16 {
            map.cell_mut(x, y).expect("inside").flags = CellFlags::empty();
        }
    }
    let mut world = world_on(map);
    let kind = slim(&mut world);
    let body = world
        .spawn_unit(kind, PlayerId(0), at(10.5, 19.5))
        .expect("spawn");
    step_with(
        &mut world,
        vec![command(
            0,
            vec![body],
            UnitOrder::Move {
                target: at(24.5, 6.5),
            },
        )],
    );
    let route = &world.entities[&body].current_funnel_waypoints;
    let corner = xy(*route.first().expect("a corner waypoint"));
    let gap = (corner.0 - 16.0).hypot(corner.1 - 16.0);
    assert!(
        (gap - (T + CORNER_MARGIN)).abs() < 1e-4,
        "corner {corner:?} is {gap} from the vertex, expected {}",
        T + CORNER_MARGIN
    );
    assert!(gap < R, "{gap}");
}

/// T-4: two slim bodies spawned overlapping on open ground separate to at least the sum of
/// their collision radii: unit-against-unit contact is unchanged.
#[test]
fn two_bodies_still_separate_at_their_full_radii() {
    let mut world = world_on(walled(20, |_, _| false));
    let kind = slim(&mut world);
    let a = world
        .spawn_unit(kind, PlayerId(0), at(10.0, 10.0))
        .expect("spawn");
    let b = world
        .spawn_unit(kind, PlayerId(0), at(10.7, 10.0))
        .expect("spawn");
    for _ in 0..40 {
        step(&mut world);
    }
    let distance = f64::from(
        world.entities[&a]
            .position
            .distance(world.entities[&b].position)
            .to_f32(),
    );
    assert!(distance >= 2.0 * R - 1e-3, "{distance}");
}

/// T-5: a `Move` to `t + ε` from the map's west edge is admitted and walked without crossing
/// `t`; one to `t − ε` is refused `OutsideDomain`. An air body's domain is still its radius.
#[test]
fn the_map_edge_admits_the_terrain_disc() {
    const EPSILON: f64 = 0.01;
    let mut world = world_on(walled(20, |_, _| false));
    let kind = slim(&mut world);
    let body = world
        .spawn_unit(kind, PlayerId(0), at(4.0, 10.0))
        .expect("spawn");
    let outside = UnitOrder::Move {
        target: at(T - EPSILON, 10.0),
    };
    assert!(matches!(
        world.try_issue_movement(PlayerId(0), &[body], &outside, false),
        Err(MovementAdmissionError::OutsideDomain(id)) if id == body
    ));
    let inside = UnitOrder::Move {
        target: at(T + EPSILON, 10.0),
    };
    world
        .try_issue_movement(PlayerId(0), &[body], &inside, false)
        .expect("the terrain disc fits");
    for _ in 0..40 {
        step(&mut world);
        assert!(xy(world.entities[&body].position).0 >= T - TOLERANCE);
    }
    let end = xy(world.entities[&body].position).0;
    assert!(
        (end - (T + EPSILON)).abs() < 0.02,
        "walked to the edge: {end}"
    );

    let air = world
        .catalog
        .archetypes()
        .find(|a| a.mover_type == MoverType::Air && a.max_speed > SimScalar::ZERO)
        .map(|a| (a.unit_type_id, f64::from(a.collision_radius.to_f32())))
        .expect("invariant: the shipped catalog has a mobile air unit");
    let flyer = world
        .spawn_unit(air.0, PlayerId(0), at(10.0, 10.0))
        .expect("spawn");
    let short = UnitOrder::Move {
        target: at(air.1 - EPSILON, 10.0),
    };
    assert!(matches!(
        world.try_issue_movement(PlayerId(0), &[flyer], &short, false),
        Err(MovementAdmissionError::OutsideDomain(id)) if id == flyer
    ));
    let fits = UnitOrder::Move {
        target: at(air.1 + EPSILON, 10.0),
    };
    world
        .try_issue_movement(PlayerId(0), &[flyer], &fits, false)
        .expect("the air disc fits");
}

/// The first structure archetype of the shipped catalog, by shape.
fn any_structure(world: &SimWorld) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has a structure")
}

/// T-6: a teleport or unload point `t + ε` west of the middle of a structure's west face is
/// admitted and one `t − ε` from it is refused `Terrain`; a second body within the sum of the
/// collision radii still refuses the admitted point as `Occupied`.
///
/// A registered structure is not a body (plan `2026-11-18-structure-contact` S-2, B-131): its
/// footprint judges the point at the terrain radius, even at the face's middle, where its old
/// circle of half its width reached `h + r` from its centre.
#[test]
fn a_placement_beside_a_structure_uses_the_terrain_radius() {
    const EPSILON: f64 = 0.01;
    let mut world = world_on(walled(32, |_, _| false));
    let kind = slim(&mut world);
    let structure = any_structure(&world);
    let placed = world
        .spawn_structure(structure, PlayerId(0), 14, 14)
        .expect("open ground");
    let footprint = &world.map.structures()[&placed];
    let left = f64::from(footprint.top_left.0);
    let y = f64::from(footprint.top_left.1) + f64::from(footprint.height) / 2.0;
    let body = world
        .spawn_unit(kind, PlayerId(0), at(4.0, 4.0))
        .expect("spawn");
    let beside = at(left - T - EPSILON, y);
    world
        .validate_displacement_destination(body, beside)
        .expect("the terrain disc clears the footprint");
    assert!(matches!(
        world.validate_displacement_destination(body, at(left - T + EPSILON, y)),
        Err(DisplacementAdmissionError::Terrain(id)) if id == body
    ));
    let other = world
        .spawn_unit(kind, PlayerId(1), at(left - T - EPSILON - 0.9, y))
        .expect("spawn");
    assert!(matches!(
        world.validate_displacement_destination(body, beside),
        Err(DisplacementAdmissionError::Occupied(id)) if id == other
    ));
}

/// T-7: a builder raises a structure beside a holding slim body that stands 0.4 from the site:
/// more than its terrain radius, less than its radius. The body does not block the site.
#[test]
fn a_body_at_its_terrain_radius_from_a_site_does_not_block_it() {
    let map = GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0))).expect("map");
    let mut world = world_on(map);
    let builder_type = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.can_construct)
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has a builder");
    let builder_arch = world.catalog.get(builder_type).expect("registered").clone();
    let (structure, width, height) = world
        .catalog
        .archetypes()
        .filter(|a| builder_arch.can_raise(a) && a.requires_structures.is_empty())
        .find(|a| a.supply_provided > 0)
        .and_then(|a| match a.shape {
            ShapeKind::BuildingDome {
                footprint_w,
                footprint_h,
                ..
            } => Some((a.unit_type_id, footprint_w, footprint_h)),
            ShapeKind::UnitSphere => None,
        })
        .expect("invariant: the builder raises a supply structure with no prerequisite");
    let (cell_x, cell_y) = (24_u32, 20_u32);
    let kind = slim(&mut world);
    let bystander = world
        .spawn_unit(
            kind,
            PlayerId(0),
            at(
                f64::from(cell_x) - 0.4,
                f64::from(cell_y) + f64::from(height) / 2.0,
            ),
        )
        .expect("spawn");
    hold(&mut world, bystander);
    let worker = world
        .spawn_unit(builder_type, PlayerId(0), at(12.5, 12.5))
        .expect("spawn");
    world.player_resources[PlayerSlot::ALL[0]].minerals = 1000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 1000;
    step_with(
        &mut world,
        vec![command(
            0,
            vec![worker],
            UnitOrder::Build {
                structure_type: structure,
                cell_x,
                cell_y,
            },
        )],
    );
    let laid = (0..600).any(|_| {
        step(&mut world);
        world.map.structures().values().any(|placed| {
            placed.top_left == (cell_x, cell_y) && placed.width == width && placed.height == height
        })
    });
    assert!(laid, "the site was laid beside the bystander");
}

/// T-9: a dasher with collision radius 0.625 and terrain radius 0.375 attacks a target beyond a
/// one-cell slot. The straight line through the slot clears its terrain disc, so the Charge
/// starts at once; with terrain radius 0.625 no walk exists, and it does not.
#[test]
fn a_charge_dash_path_is_tested_at_the_terrain_radius() {
    for (terrain, dashes) in [(0.375, true), (0.625, false)] {
        let mut world = world_on(walled(24, |x, y| x == 12 && y != 10));
        let mut blade = world
            .catalog
            .archetypes()
            .find_map(|a| a.weapon.clone())
            .expect("invariant: the shipped catalog has a weapon");
        blade.max_range = s(0.1);
        let dasher_type = register(
            &mut world,
            "terrain_radius_dasher",
            0.625,
            terrain,
            |arch| {
                arch.weapon = Some(blade);
                arch.capabilities.attack_dash = Some(AttackDashSpec {
                    speed_multiplier: s(2.0),
                    duration_ticks: 40,
                    cooldown_ticks: 300,
                    min_gap: s(0.6),
                    max_distance: s(4.0),
                });
            },
        );
        let target_type = register(&mut world, "terrain_radius_target", 0.5, 0.5, |_| {});
        let dasher = world
            .spawn_unit(dasher_type, PlayerId(0), at(10.0, 10.5))
            .expect("spawn");
        let target = world
            .spawn_unit(target_type, PlayerId(1), at(13.5, 10.5))
            .expect("spawn");
        hold(&mut world, target);
        world
            .entities
            .get_mut(&dasher)
            .expect("spawned")
            .issue_order(UnitOrder::attack(target), false)
            .expect("attack admitted");
        let mut dashed = false;
        for _ in 0..3 {
            step(&mut world);
            dashed |= world.entities[&dasher].is_attack_dashing();
        }
        assert_eq!(dashed, dashes, "terrain radius {terrain}");
    }
}

/// T-10: a two-member group whose leader has terrain radius 0.375 takes the slot only its
/// terrain disc fits as its lane, searched at that radius.
#[test]
fn a_group_lane_is_searched_at_its_leaders_terrain_radius() {
    for (terrain, through) in [(0.375, true), (0.625, false)] {
        let mut world = world_on(slot_map());
        let kind = slot_body(&mut world, terrain);
        let members = vec![
            world
                .spawn_unit(kind, PlayerId(0), at(6.5, 6.5))
                .expect("spawn"),
            world
                .spawn_unit(kind, PlayerId(0), at(6.5, 8.0))
                .expect("spawn"),
        ];
        step_with(
            &mut world,
            vec![command(
                0,
                members,
                UnitOrder::Move {
                    target: at(18.5, 6.5),
                },
            )],
        );
        step(&mut world);
        let lane = world
            .route_groups()
            .iter()
            .find_map(|group| match &group.lane {
                sim_core::movement::groups::LaneState::Lane(lane) => Some(lane.clone()),
                _ => None,
            })
            .expect("the group has a lane");
        assert_eq!(lane.radius, s(terrain));
        let top = lane
            .polyline
            .iter()
            .map(|p| xy(*p).1)
            .fold(f64::MIN, f64::max);
        assert_eq!(
            top < 8.0,
            through,
            "terrain radius {terrain}: lane top {top}"
        );
    }
}

/// Review F-2 (plan `2026-11-18-inner-radius` T5): an autonomous flee goal toward the map's west
/// edge is clamped to the body's terrain radius from it, the domain it may walk.
#[test]
fn a_flee_goal_stops_at_the_terrain_radius_from_the_map_edge() {
    let mut world = world_on(walled(20, |_, _| false));
    let kind = slim(&mut world);
    let body = world
        .spawn_unit(kind, PlayerId(0), at(3.0, 10.0))
        .expect("spawn");
    let goal = world
        .compute_entity_flee_destination(body, at(5.0, 10.0), s(6.0))
        .expect("representable")
        .expect("a flee goal");
    assert_eq!(goal, at(T, 10.0));
}

/// Review F-2 (plan `2026-11-18-inner-radius` T9): a mobile producer whose terrain disc lies on
/// the map, though its collision disc crosses the edge, may queue production.
#[test]
fn a_mobile_producer_is_bounded_by_its_terrain_radius() {
    let mut world = world_on(walled(20, |_, _| false));
    let product = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == MoverType::Ground
                && a.max_speed > SimScalar::ZERO
                && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                && a.supply_cost > 0
        })
        .map(|a| (a.slug.clone(), a.unit_type_id))
        .expect("invariant: the shipped catalog has a trainable unit");
    let producer_type = register(&mut world, "terrain_radius_producer", R, T, |arch| {
        arch.trains_units = vec![product.0.clone()];
        arch.supply_cost = 0;
        arch.supply_provided = 10;
    });
    let producer = world
        .spawn_unit(producer_type, PlayerId(0), at(0.4, 10.0))
        .expect("the terrain disc fits");
    world.player_resources[PlayerSlot::ALL[0]].minerals = 1000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 1000;
    world
        .try_queue_production(PlayerId(0), &[producer], product.1)
        .expect("the producer's terrain disc lies on the map");
}
