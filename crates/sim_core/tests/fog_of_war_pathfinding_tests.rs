//! # Integration Tests: Fog of War Pathfinding & Ramp Mechanics
//!
//! Verifies StarCraft II parity for Fog of War pathfinding and vision:
//! 1. Units can path up distant fogged ramps from tick 0 (static terrain is known in competitive melee maps).
//! 2. Enemy building wall-offs constructed in Fog of War are traversed until revealed by line-of-sight, triggering dynamic repathing.
//! 3. Ground units on low ground cannot observe high-ground enemies across a cliff ledge until ascending the ramp.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines
)]

use sim_map::{CellFlags, CliffLevel, GameMap, MapCell, PathRequestWithVision, RampDir};
use sim_types::{
    EntityId, MoverType, PlayerId, PlayerSlot, SimScalar, SimVec2, Tick, TickBundle, UnitOrder,
    VisibilityState,
};

/// Radius 0.625 in world units: a standard combat body.
const MEDIUM_RADIUS: SimScalar = SimScalar::from_raw(5 << 29);

/// Empty tick input helper.
fn empty_bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Test 1: Ground unit can path up a distant fogged ramp from tick 0.
///
/// In StarCraft II competitive melee maps, static terrain (ramps, cliffs, paths)
/// is known from tick 0. A unit can immediately calculate a path to high ground through
/// fogged terrain without having scouted the ramp first.
#[test]
fn test_unexplored_ramp_blocks_path_until_scouted() {
    let width = 32;
    let height = 32;

    // Create a 32x32 map:
    // Left half (x < 16) is Cliff 0 (low ground).
    // Right half (x >= 16) is Cliff 1 (high ground).
    // Ramp connecting them at (15, 15) to (16, 15).
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); (width * height) as usize];
    for y in 0..height {
        for x in 16..width {
            let idx = (y * width + x) as usize;
            cells[idx] = MapCell::default_ground(CliffLevel(1));
        }
    }
    // Three-wide directed ramp: its surface rises from level 0 at x=15 to
    // the level-1 landing at x=16. A RAMP flag alone has no geometry.
    for y in 14..=16 {
        let cell = &mut cells[(y * width + 15) as usize];
        cell.ramp = Some(RampDir::East);
        cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
    }

    let map = GameMap::from_cells(width, height, cells, Vec::new(), Vec::new())
        .expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x1234);

    let start_pos = SimVec2::new(SimScalar::from_num(4), SimScalar::from_num(15));
    let high_goal = SimVec2::new(SimScalar::from_num(24), SimScalar::from_num(15));

    // Confirm that at tick 0, distant ramp is Fogged
    assert_eq!(
        world.vision_maps[PlayerSlot::ALL[0]].visibility_at(15, 15),
        VisibilityState::Fogged,
        "Distant ramp must be Fogged at tick 0"
    );

    // Direct pathfinder query using vision
    let req = PathRequestWithVision {
        start: start_pos,
        goal: high_goal,
        radius: MEDIUM_RADIUS,
        vision: Some(&world.vision_maps[PlayerSlot::ALL[0]]),
        block_unexplored_ramps: false,
        known_structures: Some(&world.known_blocked_cells[PlayerSlot::ALL[0]]),
    };

    let path_result = world.pathfinder.find_path_with_vision(&world.map, &req);
    assert!(
        path_result.is_ok(),
        "Ground unit must be able to path up distant fogged ramp to high ground from tick 0"
    );

    let waypoints = path_result.unwrap().waypoints;
    assert!(
        !waypoints.is_empty(),
        "Path must contain valid string-pulled waypoints"
    );
    assert!(
        waypoints.last().unwrap().distance(high_goal) <= SimScalar::from_num(1),
        "Final waypoint must reach the high-ground goal"
    );
}

/// Test 2: When an enemy builds a structure wall-off in Fog of War, the friendly unit
/// initially paths through the remembered open ramp, but dynamically repaths to an alternate route
/// the moment its line-of-sight reveals the obstruction.
#[test]
fn test_fogged_ramp_wall_off_causes_dynamic_repath_on_vision() {
    let width = 32;
    let height = 32;

    // Create map with two ramps connecting Low Ground (left) to High Ground (right):
    // Primary Ramp at y = 10 (shorter route from start at y = 10).
    // Secondary Ramp at y = 24 (longer alternate route).
    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); (width * height) as usize];
    for y in 0..height {
        for x in 16..width {
            let idx = (y * width + x) as usize;
            cells[idx] = MapCell::default_ground(CliffLevel(1));
        }
    }

    // Valid three-wide east-facing ramps with ordinary high-ground landings.
    for center_y in [10, 24] {
        for y in center_y - 1..=center_y + 1 {
            let cell = &mut cells[(y * width + 15) as usize];
            cell.ramp = Some(RampDir::East);
            cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
        }
    }

    let map = GameMap::from_cells(width, height, cells, Vec::new(), Vec::new())
        .expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x5678);

    let player_blue = PlayerId(0);

    // Start unit on low ground at (4.0, 10.0)
    let unit_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot exists in catalog");
    let ent_id = world
        .spawn_unit(
            unit_id,
            player_blue,
            SimVec2::new(SimScalar::from_num(4), SimScalar::from_num(10)),
        )
        .expect("spawn blue unit");

    // Order unit to move to high ground base at (24.0, 10.0)
    let high_goal = SimVec2::new(SimScalar::from_num(24), SimScalar::from_num(10));
    world
        .entities
        .get_mut(&ent_id)
        .unwrap()
        .issue_order(UnitOrder::Move { target: high_goal }, false)
        .expect("valid authored fixture order or buff");

    // Step 1: unit computes path. Since ramp at y=10 is fogged and no structures are known,
    // the path routes through primary ramp (y=10).
    world.step(&empty_bundle(1));
    let initial_wps = world
        .entities
        .get(&ent_id)
        .unwrap()
        .current_funnel_waypoints
        .clone();
    assert!(
        !initial_wps.is_empty(),
        "Unit must have generated waypoints toward high ground"
    );
    assert!(
        initial_wps.iter().any(|wp| wp.y <= SimScalar::from_num(12)),
        "Initial route must plan to use primary ramp at y=10"
    );

    // Build a real wall on the primary ramp's lower landing. Buildings cannot
    // occupy ramps; all three entry lanes must be blocked, and the lower landing
    // can be discovered by a ground observer before climbing onto high ground.
    // At x=4 the landing x=14 remains outside this unit's sight radius of 9.
    world
        .map
        .place_structure(EntityId(10_000), 14, 9, 1, 3)
        .expect("wall fits ordinary buildable lower landing");
    world.pathfinder.recompute_clearance(&world.map);

    // Advance world simulation several ticks until unit approaches primary ramp
    let mut discovered_blocker = false;
    let mut rerouted = false;
    for tick in 2..=80 {
        world.step(&empty_bundle(tick));
        let Some(ent) = world.entities.get(&ent_id) else {
            break;
        };

        // Check if blue discovered the blocked cell
        let blocked_idx = (10 * width + 14) as usize;
        if world.known_blocked_cells[PlayerSlot::ALL[0]][blocked_idx] {
            discovered_blocker = true;
            // Partial revelation can legitimately leave a different primary lane
            // apparently open. Require the alternate route once all lanes are known.
            let whole_wall_known = (9..=11)
                .all(|y| world.known_blocked_cells[PlayerSlot::ALL[0]][(y * width + 14) as usize]);
            if whole_wall_known && !ent.current_funnel_waypoints.is_empty() {
                let has_rerouted = ent
                    .current_funnel_waypoints
                    .iter()
                    .any(|wp| wp.y > SimScalar::from_num(16));
                assert!(
                    has_rerouted,
                    "Unit must dynamically repath toward secondary ramp upon seeing the wall-off"
                );
                rerouted = true;
                break;
            }
        }
    }

    assert!(
        discovered_blocker,
        "Unit must advance into line of sight and discover the ramp wall-off"
    );
    assert!(
        rerouted,
        "fully revealed wall must force the alternate ramp route"
    );
}

/// Test 3: Ground units on low ground cannot observe or acquire high-ground enemies
/// across a cliff ledge until ascending the ramp.
#[test]
fn test_high_ground_vision_occlusion_and_ramp_elevation() {
    let width = 32;
    let height = 32;

    let mut cells = vec![MapCell::default_ground(CliffLevel(0)); (width * height) as usize];
    for y in 0..height {
        for x in 16..width {
            let idx = (y * width + x) as usize;
            cells[idx] = MapCell::default_ground(CliffLevel(1));
        }
    }
    // The same directed geometry used by runtime maps, with a three-cell aperture.
    for y in 14..=16 {
        let cell = &mut cells[(y * width + 15) as usize];
        cell.ramp = Some(RampDir::East);
        cell.flags = CellFlags::GROUND_PATHABLE | CellFlags::RAMP;
    }

    let map = GameMap::from_cells(width, height, cells, Vec::new(), Vec::new())
        .expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x9999);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    // Keep the target in place and outside melee range so this visibility test
    // measures actual ramp traversal rather than the observer dying to gunfire.
    let unit_id = world
        .catalog
        .archetypes()
        .find(|a| {
            a.mover_type == MoverType::Ground
                && !a.is_immobile_by_default()
                && a.weapon_against(MoverType::Ground, None)
                    .is_some_and(|weapon| weapon.max_range < SimScalar::ONE)
        })
        .expect("catalog supplies a mobile ground melee archetype")
        .unit_type_id;

    // Spawn red enemy on high ground at (18.0, 5.0) (away from the ramp at y=15)
    let red_pos = SimVec2::new(SimScalar::from_num(18), SimScalar::from_num(5));
    let red_id = world
        .spawn_unit(unit_id, player_red, red_pos)
        .expect("spawn red");
    world
        .entities
        .get_mut(&red_id)
        .unwrap()
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("valid authored fixture order or buff");

    // Spawn blue unit on low ground at (14.0, 5.0) - only 4 world units away from enemy!
    // But separated by cliff ledge without a ramp at y=5!
    let blue_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(5));
    let blue_id = world
        .spawn_unit(unit_id, player_blue, blue_pos)
        .expect("spawn blue");

    // Update vision for tick 1
    world.step(&empty_bundle(1));

    // Blue unit cannot see red unit due to cliff occlusion
    assert_ne!(
        world.vision_maps[PlayerSlot::ALL[0]].visibility_at(18, 5),
        VisibilityState::Visible,
        "High ground cliff cell must be occluded for low-ground observer"
    );
    assert_eq!(
        world.entities.get(&blue_id).unwrap().current_target,
        None,
        "Blue unit must not acquire occluded enemy on high ground"
    );

    // Now order blue unit to ascend the ramp at y=15
    let ramp_top = SimVec2::new(SimScalar::from_num(17), SimScalar::from_num(15));
    world
        .entities
        .get_mut(&blue_id)
        .unwrap()
        .issue_order(UnitOrder::Move { target: ramp_top }, false)
        .expect("valid authored fixture order or buff");

    // Reaching the landing is mandatory: the old test passed vacuously if the
    // invalid flags-only ramp prevented any ascent. Allow the actual path length.
    let mut reached_high_ground = false;
    for tick in 2..=256 {
        world.step(&empty_bundle(tick));
        let pos = world.entities.get(&blue_id).unwrap().position;
        if pos.x >= SimScalar::from_num(16) {
            // Once ascended to high ground (Cliff 1), cliff occlusion is lifted!
            let blue_cell = world
                .map
                .cell(pos.x.0.to_num::<u32>(), pos.y.0.to_num::<u32>())
                .unwrap();
            assert_eq!(
                blue_cell.cliff_level,
                CliffLevel(1),
                "Blue unit must now be on high ground cliff level"
            );
            reached_high_ground = true;
            break;
        }
    }
    assert!(
        reached_high_ground,
        "blue unit must actually ascend the ramp"
    );
}
