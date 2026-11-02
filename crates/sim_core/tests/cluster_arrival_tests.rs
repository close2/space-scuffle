//! Real command/replay regressions for terrain-connected group arrival.
//! Catalog capabilities select generic controlled actors; loaded authored terrain,
//! registered foundations, supply and transport lifecycles remain authoritative.

use sim_core::{catalog::RuntimeCatalogs, SimWorld};
use sim_map::{CliffLevel, GameMap, MapCell, MapFile, PathRequest, UnitClearance};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
const SEED: u64 = 0x52_2026_1028;

fn center(x: i32, y: i32) -> SimVec2 {
    SimVec2::from_i32(x, y) + SimVec2::new(SimScalar::HALF, SimScalar::HALF)
}
fn canyon() -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(MapFile::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps/showcase_canyon.scmap.ron"),
    )?
    .decode()?
    .to_game_map()?)
}
fn flat() -> Result<GameMap, Box<dyn std::error::Error>> {
    Ok(GameMap::new(
        64,
        64,
        MapCell::default_ground(CliffLevel(0)),
    )?)
}
fn world(
    map: GameMap,
    layer: MoverType,
    fog: bool,
) -> Result<(SimWorld, UnitTypeId), Box<dyn std::error::Error>> {
    let mut catalogs = RuntimeCatalogs::load_from_directory(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog"),
    )?;
    let kind = catalogs
        .units
        .archetypes()
        .find(|a| {
            a.mover_type == layer
                && !a.is_immobile_by_default()
                && !matches!(a.shape, ShapeKind::BuildingDome { .. })
        })
        .ok_or("catalog requires mobile capability")?
        .unit_type_id;
    let arch = catalogs.units.get_mut(kind)?;
    arch.collision_radius = SimScalar::HALF;
    arch.max_speed = SimScalar::from_num(0.25);
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    let mut world = SimWorld::with_catalogs(map, SEED, catalogs.units, catalogs.upgrades);
    world.vision_aware_pathfinding = fog;
    Ok((world, kind))
}
fn order(attack: bool, target: SimVec2) -> UnitOrder {
    if attack {
        UnitOrder::AttackMove { target }
    } else {
        UnitOrder::Move { target }
    }
}
fn command(ids: &[EntityId], attack: bool, target: SimVec2, queue: bool) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(0),
        entities: ids.to_vec(),
        order: order(attack, target),
        queue,
    }
}
fn tick(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}
fn pair(
    world: &mut SimWorld,
    kind: UnitTypeId,
    start: SimVec2,
    witness: SimVec2,
) -> Result<[EntityId; 2], Box<dyn std::error::Error>> {
    Ok([
        world.spawn_unit(kind, PlayerId(0), start)?,
        world.spawn_unit(kind, PlayerId(0), witness)?,
    ])
}

#[test]
fn authored_cliff_groups_follow_real_ramp_and_replay_exactly() -> TestResult {
    for attack in [false, true] {
        for (reverse, same_side, fog) in [
            (false, false, false),
            (false, true, false),
            (false, false, true),
            (false, true, true),
            (true, false, false),
        ] {
            let (start, goal) = if reverse {
                (center(26, 1), center(25, 1))
            } else {
                (center(25, 1), center(26, 1))
            };
            let witness = if same_side { center(25, 2) } else { goal };
            let map = canyon()?;
            assert!(!map.ground_segment_is_connected(start, goal, |_, _| true));
            assert_eq!(
                map.ground_segment_is_connected(start, witness, |_, _| true),
                same_side
            );
            assert_eq!(
                map.ground_segment_is_connected(witness, goal, |_, _| true),
                !same_side
            );
            let (mut first, kind) = world(map.clone(), MoverType::Ground, fog)?;
            let (mut second, second_kind) = world(map, MoverType::Ground, fog)?;
            let path = first.pathfinder.find_path(
                &first.map,
                &PathRequest {
                    start,
                    goal,
                    clearance: UnitClearance::Medium,
                },
            )?;
            assert!(path.waypoints.iter().any(|p| p.y >= SimScalar::from_num(8)));
            let ids = pair(&mut first, kind, start, witness)?;
            assert_eq!(ids, pair(&mut second, second_kind, start, witness)?);
            let supply = first.player_resources[0].supply_used;
            let mut reached = false;
            for frame in 0..256 {
                let before = ids.map(|id| first.entities[&id].position);
                let commands = if frame == 0 {
                    vec![command(&ids, attack, goal, false)]
                } else {
                    vec![]
                };
                tick(&mut first, commands.clone());
                tick(&mut second, commands);
                if frame == 0 {
                    assert_eq!(first.movement_outcomes().len(), 1);
                    assert!(first.movement_outcomes()[0].result.is_ok());
                    assert!(
                        !first.entities[&ids[0]].order_queue.is_empty(),
                        "false cliff arrival"
                    );
                }
                assert_eq!(first.last_checksum(), first.compute_checksum());
                assert_eq!(
                    first.last_checksum(),
                    second.last_checksum(),
                    "frame {frame}"
                );
                assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
                assert_eq!(second.diagnostics(), []);
                first.validate_supply_ledger()?;
                assert_eq!(first.player_resources[0].supply_used, supply);
                for (id, previous) in ids.into_iter().zip(before) {
                    assert!(first.map.ground_segment_is_connected(
                        previous,
                        first.entities[&id].position,
                        |_, _| true
                    ));
                }
                let actor = &first.entities[&ids[0]];
                reached |= actor.order_queue.is_empty()
                    && first
                        .map
                        .ground_segment_is_connected(actor.position, goal, |_, _| true)
                    && actor.position.distance(goal) < SimScalar::from_num(2.5);
            }
            assert!(
                reached,
                "attack={attack}, reverse={reverse}, same_side={same_side}, fog={fog}"
            );
        }
    }
    Ok(())
}

#[test]
fn flat_and_air_keep_arrival_and_pop_only_the_current_order() -> TestResult {
    for layer in [MoverType::Ground, MoverType::Air] {
        for attack in [false, true] {
            let map = if layer == MoverType::Air {
                canyon()?
            } else {
                flat()?
            };
            let (mut w, kind) = world(map, layer, false)?;
            let start = center(25, 1);
            let goal = center(26, 1);
            let follow = center(30, 1);
            let ids = pair(&mut w, kind, start, goal)?;
            tick(
                &mut w,
                vec![
                    command(&ids, attack, goal, false),
                    command(&ids, !attack, follow, true),
                ],
            );
            assert_eq!(w.movement_outcomes().len(), 2);
            assert!(w.movement_outcomes().iter().all(|o| o.result.is_ok()));
            // The unit on the goal is done at once and walks off to its queued order, so the
            // other one settles on the goal itself. Only the first order may end either way.
            for _ in 0..16 {
                if w.entities[&ids[0]].order_queue.len() == 1 {
                    break;
                }
                tick(&mut w, Vec::new());
            }
            assert_eq!(w.entities[&ids[0]].order_queue.len(), 1);
            assert_eq!(
                w.entities[&ids[0]].order_queue.front(),
                Some(&order(!attack, follow))
            );
            assert_eq!(w.diagnostics(), []);
            w.validate_supply_ledger()?;
        }
    }
    Ok(())
}

#[test]
fn disconnected_arrival_cannot_pop_a_queued_followup() -> TestResult {
    for attack in [false, true] {
        let (mut w, kind) = world(canyon()?, MoverType::Ground, false)?;
        let start = center(25, 1);
        let goal = center(26, 1);
        let ids = pair(&mut w, kind, start, goal)?;
        tick(
            &mut w,
            vec![
                command(&ids, attack, goal, false),
                command(&ids, !attack, center(29, 1), true),
            ],
        );
        assert!(w.movement_outcomes().iter().all(|o| o.result.is_ok()));
        assert_eq!(w.entities[&ids[0]].order_queue.len(), 2);
        assert_eq!(
            w.entities[&ids[0]].order_queue.front(),
            Some(&order(attack, goal))
        );
        assert_eq!(w.diagnostics(), []);
    }
    Ok(())
}

#[test]
fn order_onto_a_foundation_ends_beside_it_without_revealing_it() -> TestResult {
    for fog in [false, true] {
        for attack in [false, true] {
            let (mut w, kind) = world(flat()?, MoverType::Ground, fog)?;
            let dome = w
                .catalog
                .archetypes()
                .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
                .ok_or("catalog requires foundation capability")?
                .unit_type_id;
            let arch = w.catalog.get_mut(dome)?;
            arch.shape = ShapeKind::BuildingDome {
                footprint_w: 1,
                footprint_h: 1,
            };
            arch.collision_radius = SimScalar::HALF;
            arch.weapon = None;
            arch.ground_weapon = None;
            arch.air_weapon = None;
            let foundation = w.spawn_structure(dome, PlayerId(0), 10, 10)?;
            let goal = w.entities[&foundation].position;
            assert_eq!(goal, center(10, 10));
            let placement = w
                .map
                .structures()
                .get(&foundation)
                .cloned()
                .ok_or("registered foundation")?;
            let actor = w.spawn_unit(kind, PlayerId(0), center(9, 10))?;
            assert!(w
                .map
                .ground_segment_is_connected(center(9, 10), goal, |_, _| true));
            assert!(!SimWorld::is_position_pathable(&w.map, center(9, 10), goal));
            // Moving must not discover the registered structure through a knowledge side
            // channel. Exercise actual unknown then revealed masks; the normal tick below
            // owns legitimate vision publication.
            assert!(!w.known_blocked_cells[0][10 * 64 + 10]);
            w.try_issue_movement(PlayerId(0), &[actor], &order(attack, goal), false)?;
            let unknown = w.known_blocked_cells.clone();
            w.step_movement_and_steering();
            assert_eq!(w.known_blocked_cells, unknown);
            w.entities.get_mut(&actor).ok_or("actor")?.position = center(9, 10);
            tick(&mut w, vec![command(&[actor], attack, goal, false)]);
            assert!(w.movement_outcomes()[0].result.is_ok());
            assert!(w.known_blocked_cells[0][10 * 64 + 10]);
            // Once seen, the foundation is not a destination: the nearest point is, and the
            // actor walks there and stops against nothing but its own arrival.
            let revealed = w.known_blocked_cells.clone();
            w.try_issue_movement(PlayerId(0), &[actor], &order(attack, goal), false)?;
            for _ in 0..40 {
                w.step_movement_and_steering();
                if w.entities[&actor].order_queue.is_empty() {
                    break;
                }
            }
            assert_eq!(w.known_blocked_cells, revealed);
            assert!(w.entities[&actor].order_queue.is_empty());
            assert!(!SimWorld::is_position_pathable(
                &w.map,
                w.entities[&actor].position,
                goal
            ));
            assert_eq!(w.entities[&foundation].position, goal);
            assert_eq!(w.map.structures().get(&foundation), Some(&placement));
            assert!(w.diagnostics().is_empty(), "{:?}", w.diagnostics());
            w.validate_supply_ledger()?;
        }
    }
    Ok(())
}

#[test]
fn dead_cargo_and_other_layer_neighbors_cannot_complete_arrival() -> TestResult {
    for case in 0..3 {
        let (mut w, kind) = world(flat()?, MoverType::Ground, false)?;
        let start = center(20, 20);
        let goal = center(21, 20);
        let actor = w.spawn_unit(kind, PlayerId(0), start)?;
        let witness_kind = if case == 2 {
            let air = w
                .catalog
                .archetypes()
                .find(|a| {
                    a.mover_type == MoverType::Air
                        && !a.is_immobile_by_default()
                        && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                })
                .ok_or("air capability")?
                .unit_type_id;
            let arch = w.catalog.get_mut(air)?;
            arch.collision_radius = SimScalar::HALF;
            arch.weapon = None;
            arch.ground_weapon = None;
            arch.air_weapon = None;
            air
        } else {
            kind
        };
        let witness = w.spawn_unit(witness_kind, PlayerId(0), goal)?;
        match case {
            0 => w.entities.get_mut(&witness).ok_or("witness")?.hp = SimScalar::ZERO,
            1 => {
                let carrier = w.spawn_unit(kind, PlayerId(0), center(40, 40))?;
                w.try_load_unit_into_transport(witness, carrier)?;
            }
            _ => {}
        }
        tick(&mut w, vec![command(&[actor], false, goal, false)]);
        assert!(w.movement_outcomes()[0].result.is_ok());
        assert!(!w.entities[&actor].order_queue.is_empty());
        assert_ne!(w.entities[&actor].position, start);
        assert!(w.diagnostics().is_empty(), "{:?}", w.diagnostics());
        w.validate_supply_ledger()?;
    }
    Ok(())
}

#[test]
fn eight_actor_ramp_motion_preserves_terrain_and_full_replay_checksums() -> TestResult {
    let (mut first, kind) = world(canyon()?, MoverType::Ground, false)?;
    let (mut second, second_kind) = world(canyon()?, MoverType::Ground, false)?;
    let mut ids = Vec::new();
    for row in 0..3 {
        for col in 0..3 {
            if row == 2 && col == 2 {
                continue;
            }
            let position = center(23 + col, 1 + row);
            let id = first.spawn_unit(kind, PlayerId(0), position)?;
            assert_eq!(id, second.spawn_unit(second_kind, PlayerId(0), position)?);
            ids.push(id);
        }
    }
    let initial: Vec<_> = ids.iter().map(|id| first.entities[id].position).collect();
    let mut moved = vec![false; ids.len()];
    for frame in 0..256 {
        let previous: Vec<_> = ids.iter().map(|id| first.entities[id].position).collect();
        let commands = if frame == 0 {
            vec![command(&ids, false, center(26, 1), false)]
        } else {
            vec![]
        };
        tick(&mut first, commands.clone());
        tick(&mut second, commands);
        assert_eq!(first.last_checksum(), second.last_checksum());
        assert_eq!(first.last_checksum(), first.compute_checksum());
        assert!(first.diagnostics().is_empty(), "{:?}", first.diagnostics());
        for (index, id) in ids.iter().enumerate() {
            let position = first.entities[id].position;
            assert!(first
                .map
                .ground_segment_is_connected(previous[index], position, |_, _| true));
            moved[index] |= position.distance(initial[index]) > SimScalar::HALF;
        }
        first.validate_supply_ledger()?;
    }
    assert!(
        moved.iter().all(|value| *value),
        "every actor must make actual progress"
    );
    Ok(())
}
