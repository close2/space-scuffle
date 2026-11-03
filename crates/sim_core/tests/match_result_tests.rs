//! Win and loss: every way of losing a last structure, and what follows.
//!
//! # Responsibility
//! Checks the rules in `match_state` on the tick API: who is defeated, when the match ends,
//! that the result is authoritative and published, and how commands are refused afterwards.
//!
//! # Invariants
//! Structures are destroyed by real damage paths where the path is the point. Where only the
//! outcome matters, setup zeroes hit points and the reap path does the rest.

use sim_core::economy::{GameCommandError, GameCommandReceipt};
use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    EntityId, MatchResult, PlayerCommand, PlayerId, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};
use std::error::Error;

const RED: PlayerId = PlayerId(0);
const BLUE: PlayerId = PlayerId(1);
const GREEN: PlayerId = PlayerId(2);

type TestResult = Result<(), Box<dyn Error>>;

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

fn world() -> Result<(SimWorld, UnitTypeId), Box<dyn Error>> {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let world = sim_fixtures::world(map, 41);
    let dome = world
        .catalog
        .id_by_slug("capacitor_dome")
        .ok_or("shipped structure")?;
    Ok((world, dome))
}

fn step(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
}

fn raze(world: &mut SimWorld, ids: &[EntityId]) -> TestResult {
    for id in ids {
        world.entities.get_mut(id).ok_or("target")?.hp = SimScalar::ZERO;
    }
    step(world, Vec::new());
    Ok(())
}

#[test]
fn losing_the_last_structure_defeats_and_the_last_player_wins() -> TestResult {
    let (mut world, dome) = world()?;
    let first = world.spawn_structure(dome, RED, 2, 2)?;
    let second = world.spawn_structure(dome, RED, 6, 2)?;
    world.spawn_structure(dome, BLUE, 30, 30)?;
    step(&mut world, Vec::new());
    assert_eq!(world.match_state.result, MatchResult::InProgress);

    raze(&mut world, &[first])?;
    assert_eq!(
        world.match_state.result,
        MatchResult::InProgress,
        "one left"
    );
    assert!(!world.match_state.is_defeated(RED));

    raze(&mut world, &[second])?;
    assert!(world.match_state.is_defeated(RED));
    let tick = Tick(world.current_tick);
    assert_eq!(
        world.match_state.result,
        MatchResult::Victory { winner: BLUE, tick }
    );
    Ok(())
}

#[test]
fn a_standing_army_does_not_save_a_player_without_structures() -> TestResult {
    let (mut world, dome) = world()?;
    let hall = world.spawn_structure(dome, RED, 2, 2)?;
    world.spawn_structure(dome, BLUE, 30, 30)?;
    let soldier = world.catalog.id_by_slug("pulse_bot").ok_or("unit")?;
    world.spawn_unit(soldier, RED, at(4, 8))?;
    step(&mut world, Vec::new());
    raze(&mut world, &[hall])?;
    assert!(world.match_state.is_defeated(RED));
    assert!(world.match_state.result.is_over());
    Ok(())
}

#[test]
fn a_foundation_counts_as_a_structure() -> TestResult {
    let (mut world, dome) = world()?;
    let hall = world.spawn_structure(dome, RED, 2, 2)?;
    world.spawn_structure(dome, BLUE, 30, 30)?;
    let builder = world
        .catalog
        .archetypes()
        .find(|a| a.capabilities.can_construct)
        .map(|a| a.unit_type_id)
        .ok_or("no builder")?;
    let worker = world.spawn_unit(builder, RED, at(10, 10))?;
    step(
        &mut world,
        vec![command(
            RED,
            vec![worker],
            UnitOrder::Build {
                structure_type: dome,
                cell_x: 12,
                cell_y: 10,
            },
        )],
    );
    for _ in 0..200 {
        step(&mut world, Vec::new());
    }
    let foundation = world
        .entities
        .values()
        .find(|e| e.construction.is_some())
        .map(|e| e.id)
        .ok_or("foundation laid")?;
    raze(&mut world, &[hall])?;
    assert_eq!(world.match_state.result, MatchResult::InProgress);
    assert!(!world.match_state.is_defeated(RED));

    // Cancelling is a structure loss like any other.
    step(
        &mut world,
        vec![command(
            RED,
            vec![foundation],
            UnitOrder::CancelConstruction,
        )],
    );
    assert!(world.match_state.is_defeated(RED));
    assert!(world.match_state.result.is_over());
    Ok(())
}

fn command(player: PlayerId, entities: Vec<EntityId>, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities,
        order,
        queue: false,
    }
}

#[test]
fn losing_the_last_structure_to_real_weapon_fire_ends_the_match() -> TestResult {
    let (mut world, dome) = world()?;
    let target = world.spawn_structure(dome, RED, 10, 10)?;
    world.spawn_structure(dome, BLUE, 40, 40)?;
    let soldier = world.catalog.id_by_slug("pulse_bot").ok_or("unit")?;
    let mut squad = Vec::new();
    for y in 0..4 {
        squad.push(world.spawn_unit(soldier, BLUE, at(14, 9 + y))?);
    }
    step(&mut world, Vec::new());
    world.entities.get_mut(&target).ok_or("target")?.hp = SimScalar::from_num(30);
    step(
        &mut world,
        vec![command(BLUE, squad, UnitOrder::attack(target))],
    );
    for _ in 0..400 {
        if world.match_state.result.is_over() {
            break;
        }
        step(&mut world, Vec::new());
    }
    assert!(matches!(
        world.match_state.result,
        MatchResult::Victory { winner, .. } if winner == BLUE
    ));
    Ok(())
}

#[test]
fn falling_together_is_a_draw() -> TestResult {
    let (mut world, dome) = world()?;
    let red = world.spawn_structure(dome, RED, 2, 2)?;
    let blue = world.spawn_structure(dome, BLUE, 30, 30)?;
    step(&mut world, Vec::new());
    raze(&mut world, &[red, blue])?;
    assert_eq!(
        world.match_state.result,
        MatchResult::Draw {
            tick: Tick(world.current_tick)
        }
    );
    Ok(())
}

#[test]
fn in_a_larger_match_a_defeated_player_loses_command_while_others_play() -> TestResult {
    let (mut world, dome) = world()?;
    let red = world.spawn_structure(dome, RED, 2, 2)?;
    world.spawn_structure(dome, BLUE, 30, 2)?;
    world.spawn_structure(dome, GREEN, 30, 30)?;
    let soldier = world.catalog.id_by_slug("pulse_bot").ok_or("unit")?;
    let survivor = world.spawn_unit(soldier, RED, at(4, 10))?;
    let other = world.spawn_unit(soldier, BLUE, at(34, 10))?;
    step(&mut world, Vec::new());
    raze(&mut world, &[red])?;
    assert_eq!(world.match_state.result, MatchResult::InProgress);

    step(
        &mut world,
        vec![
            command(RED, vec![survivor], UnitOrder::HoldPosition),
            command(BLUE, vec![other], UnitOrder::HoldPosition),
        ],
    );
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err(GameCommandError::PlayerDefeated(RED))
    );
    assert_eq!(
        world.entities[&other].order_queue.front(),
        Some(&UnitOrder::HoldPosition)
    );
    assert_ne!(
        world.entities[&survivor].order_queue.front(),
        Some(&UnitOrder::HoldPosition)
    );
    Ok(())
}

#[test]
fn a_world_without_two_structure_owners_never_ends() -> TestResult {
    let (mut world, dome) = world()?;
    let only = world.spawn_structure(dome, RED, 2, 2)?;
    let soldier = world.catalog.id_by_slug("pulse_bot").ok_or("unit")?;
    world.spawn_unit(soldier, BLUE, at(20, 20))?;
    step(&mut world, Vec::new());
    raze(&mut world, &[only])?;
    assert_eq!(world.match_state.result, MatchResult::InProgress);
    assert!(!world.match_state.is_defeated(RED));
    Ok(())
}

#[test]
fn the_result_is_part_of_the_checksum_and_the_snapshot() -> TestResult {
    let (mut world, dome) = world()?;
    let red = world.spawn_structure(dome, RED, 2, 2)?;
    world.spawn_structure(dome, BLUE, 30, 30)?;
    step(&mut world, Vec::new());
    raze(&mut world, &[red])?;

    let snapshot = world.build_render_snapshot();
    assert_eq!(snapshot.match_result, world.match_state.result);
    assert!(snapshot.players[0].defeated);
    assert!(!snapshot.players[1].defeated);

    let before = world.compute_checksum();
    world.match_state.result = MatchResult::InProgress;
    assert_ne!(world.compute_checksum(), before);
    let reset = world.compute_checksum();
    world.match_state.defeated = 0;
    assert_ne!(world.compute_checksum(), reset);
    Ok(())
}

#[test]
fn the_result_never_changes_once_set() -> TestResult {
    let (mut world, dome) = world()?;
    let red = world.spawn_structure(dome, RED, 2, 2)?;
    let blue = world.spawn_structure(dome, BLUE, 30, 30)?;
    step(&mut world, Vec::new());
    raze(&mut world, &[red])?;
    let result = world.match_state.result;
    raze(&mut world, &[blue])?;
    assert_eq!(world.match_state.result, result);
    assert!(!world.match_state.is_defeated(BLUE));
    Ok(())
}

#[test]
fn commands_of_any_kind_are_refused_after_the_end() -> TestResult {
    let (mut world, dome) = world()?;
    let red = world.spawn_structure(dome, RED, 2, 2)?;
    let blue = world.spawn_structure(dome, BLUE, 30, 30)?;
    step(&mut world, Vec::new());
    raze(&mut world, &[red])?;
    step(
        &mut world,
        vec![command(
            BLUE,
            vec![blue],
            UnitOrder::Move { target: at(4, 4) },
        )],
    );
    assert_eq!(world.movement_outcomes(), []);
    assert_eq!(
        world.game_command_outcomes()[0].result,
        Err::<GameCommandReceipt, _>(GameCommandError::MatchOver)
    );
    Ok(())
}
