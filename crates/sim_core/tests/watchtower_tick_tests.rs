//! Watchtower capture and sight through the real tick pipeline.
//!
//! # Responsibility
//! Proves towers are live gameplay rather than data: a unit walked onto a tower by a
//! `PlayerCommand` grants its owner the tower's vision, and leaving gives it up.
//!
//! # How it fits
//! `sim_map` owns the capture rule and `SimWorld` must drive it each tick. These tests cover
//! that wiring, which is what was missing: the rule existed but nothing called it.
//!
//! # Invariants
//! Everything runs on the shipped default arena through `SimWorld::step`.

use sim_core::SimWorld;
use sim_map::{MapFile, VisibilityState};
use sim_types::{
    EntityId, MoverType, PlayerCommand, PlayerId, SimVec2, Tick, TickBundle, UnitOrder,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const UNIT: &str = "pulse_bot";

/// The shipped default arena, loaded from `assets/maps/default_arena.scmap.ron`.
fn arena() -> Result<SimWorld, Box<dyn std::error::Error>> {
    let map = MapFile::load(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../assets/maps/default_arena.scmap.ron"),
    )?
    .decode()?
    .to_game_map()?;
    Ok(SimWorld::new(map, 11))
}

fn spawn(
    world: &mut SimWorld,
    slug: &str,
    player: u8,
    pos: SimVec2,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let unit_type = world
        .catalog
        .id_by_slug(slug)
        .ok_or("unit not in catalog")?;
    Ok(world.spawn_unit(unit_type, PlayerId(player), pos)?)
}

fn step(world: &mut SimWorld, tick: &mut u32, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(*tick),
        commands,
        state_checksum: None,
    });
    *tick += 1;
}

fn tower_position(world: &SimWorld) -> Result<SimVec2, Box<dyn std::error::Error>> {
    Ok(world
        .map
        .watchtowers()
        .first()
        .ok_or("arena has a tower")?
        .pos)
}

fn controller(world: &SimWorld) -> Option<PlayerId> {
    world
        .map
        .watchtowers()
        .first()
        .and_then(|tower| tower.controlling_player)
}

#[test]
fn f2_unit_walking_onto_a_tower_captures_it_and_gains_vision() -> TestResult {
    let mut world = arena()?;
    let tower = tower_position(&world)?;
    let unit = spawn(&mut world, UNIT, 0, tower + SimVec2::from_i32(-12, -2))?;
    let far_cell = (28, 38);

    let mut tick = 0;
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(controller(&world), None);
    assert_ne!(
        world.vision_maps[0].visibility_at(far_cell.0, far_cell.1),
        VisibilityState::Visible
    );

    let order = PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![unit],
        order: UnitOrder::Move { target: tower },
        queue: false,
    };
    step(&mut world, &mut tick, vec![order]);
    for _ in 0..400 {
        step(&mut world, &mut tick, Vec::new());
        if controller(&world).is_some() {
            break;
        }
    }

    assert_eq!(controller(&world), Some(PlayerId(0)));
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(
        world.vision_maps[0].visibility_at(far_cell.0, far_cell.1),
        VisibilityState::Visible,
        "the tower must grant its reveal radius"
    );
    Ok(())
}

#[test]
fn f2_two_players_on_a_tower_contest_it() -> TestResult {
    let mut world = arena()?;
    let tower = tower_position(&world)?;
    spawn(&mut world, UNIT, 0, tower)?;
    let mut tick = 0;
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(controller(&world), Some(PlayerId(0)));

    spawn(&mut world, UNIT, 1, tower + SimVec2::from_i32(1, 0))?;
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(controller(&world), None);
    Ok(())
}

#[test]
fn f2_tower_is_released_when_its_occupant_dies() -> TestResult {
    let mut world = arena()?;
    let tower = tower_position(&world)?;
    let unit = spawn(&mut world, UNIT, 0, tower)?;
    let mut tick = 0;
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(controller(&world), Some(PlayerId(0)));

    world.entities.remove(&unit);
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(controller(&world), None);
    Ok(())
}

#[test]
fn f2_air_units_do_not_capture_towers() -> TestResult {
    let mut world = arena()?;
    let tower = tower_position(&world)?;
    let air = world
        .catalog
        .archetypes()
        .find(|archetype| archetype.mover_type == MoverType::Air)
        .map(|archetype| archetype.slug.clone())
        .ok_or("catalog has no air unit")?;
    spawn(&mut world, &air, 0, tower)?;
    let mut tick = 0;
    step(&mut world, &mut tick, Vec::new());
    assert_eq!(controller(&world), None);
    Ok(())
}
