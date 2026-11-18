//! # Integration Test: ranges to a structure measure to its engine radius (B-135)
//!
//! # Responsibility
//! Plan `2026-11-18-structure-radius`: a structure's `collision_radius` is its authored
//! `radius`, the analogue's 5.0.14 engine radius, and every range reads it, while its terrain
//! radius is half its footprint's smaller side and its dome is drawn at its footprint:
//! - R-12: a 5×5 whose radius exceeds its half-width keeps a valid motion entry when it stands
//!   flush with the map's corner (D-5).
//!
//! # How it fits
//! Plays `SimWorld` on small synthetic maps with the shipped catalogs. A structure under test is
//! a scratch copy of a shipped structure, chosen by footprint size, loaded through the catalog's
//! own RON path (`UnitCatalog::load_structures_from_ron_str`) with its `radius` replaced, so it
//! passes the loader exactly as a shipped row does. Mobile bodies are scratch clones of a shipped
//! mobile unit, as in `structure_contact_tests`. Nothing is chosen by name (charter §3.3).
//!
//! # Invariants
//! - A structure is found by `ShapeKind::BuildingDome` and its footprint size only.
//! - Every tick of `step` ends with no simulation diagnostic.

#![allow(clippy::expect_used, clippy::too_many_lines)]

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
    sim_fixtures::world(map, 0x0B_135)
}

/// A unit type id no archetype of `world`'s catalog uses.
fn free_id(world: &SimWorld) -> UnitTypeId {
    (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("invariant: a free unit type id")
}

/// The shipped `structures.ron` split into its structure blocks, each the lines from its opening
/// `(` to its closing `),` at the list's indentation.
fn structure_blocks() -> Vec<Vec<&'static str>> {
    let mut blocks = Vec::new();
    let mut current: Option<Vec<&str>> = None;
    for line in sim_fixtures::STRUCTURES_RON.lines() {
        if line == "        (" {
            current = Some(vec![line]);
        } else if let Some(block) = current.as_mut() {
            block.push(line);
            if line == "        )," {
                blocks.extend(current.take());
            }
        }
    }
    blocks
}

/// Loads a scratch copy of the first shipped structure whose footprint is `size` × `size` and
/// for which `pick` holds, under a free id and the slug `slug`, with its `radius` set to `radius`
/// (world units, as authored), through the catalog's RON loader. Returns its unit type id.
fn scratch_structure(
    world: &mut SimWorld,
    size: u32,
    radius: f64,
    slug: &str,
    pick: impl Fn(&UnitArchetype) -> bool,
) -> UnitTypeId {
    let footprint = format!("footprint: ({size}, {size}),");
    let block = structure_blocks()
        .into_iter()
        .find(|block| {
            block.iter().any(|line| line.trim() == footprint)
                && block
                    .iter()
                    .find_map(|line| line.trim().strip_prefix("slug: "))
                    .and_then(|slug| {
                        world
                            .catalog
                            .get_by_slug(slug.trim_end_matches(',').trim_matches('"'))
                    })
                    .is_some_and(&pick)
        })
        .expect("invariant: the shipped catalog has such a structure");
    let id = free_id(world);
    let edited: Vec<String> = block
        .iter()
        .map(|line| {
            let trimmed = line.trim_start();
            let indent = &line[..line.len() - trimmed.len()];
            if trimmed.starts_with("structure_type_id:") {
                format!("{indent}structure_type_id: {},", id.0)
            } else if trimmed.starts_with("slug:") {
                format!("{indent}slug: \"{slug}\",")
            } else if trimmed.starts_with("name:") {
                format!("{indent}name: \"{slug}\",")
            } else if trimmed.starts_with("radius:") {
                format!("{indent}radius: {radius},")
            } else {
                (*line).to_owned()
            }
        })
        .collect();
    let text = format!("(structures: [\n{}\n])", edited.join("\n"));
    world
        .catalog
        .load_structures_from_ron_str(&text)
        .expect("the scratch structure loads");
    id
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
    let free = free_id(world);
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

fn unarmed(archetype: &UnitArchetype) -> bool {
    archetype.weapon.is_none()
        && archetype.ground_weapon.is_none()
        && archetype.air_weapon.is_none()
}

/// R-12: an unarmed 5×5 with radius 2.75, a quarter more than its half-width, registered at
/// cells (0, 0) so its centre is 2.5 from two map edges, and a unit ordered past it, are stepped
/// 25 ticks with no diagnostic: the per-tick motion-entry domain check of the foundation reads
/// its terrain radius, half its footprint (D-5), not its radius.
#[test]
fn a_structure_flush_with_the_map_edge_keeps_a_valid_motion_entry() {
    let mut world = world_on(flat(32));
    let kind = scratch_structure(&mut world, 5, 2.75, "structure_radius_flush", unarmed);
    let arch = world.catalog.get(kind).expect("loaded");
    assert_eq!(arch.collision_radius, s(2.75));
    assert_eq!(arch.terrain_radius, s(2.5));
    world
        .spawn_structure(kind, PlayerId(0), 0, 0)
        .expect("open ground at the corner");
    let walker = register(&mut world, "structure_radius_walker", 0.5, 0.375, |_| {});
    let body = world
        .spawn_unit(walker, PlayerId(0), at(9.0, 2.0))
        .expect("spawn");
    step_with(&mut world, vec![move_to(0, body, at(2.0, 9.0))]);
    for _ in 0..24 {
        step(&mut world);
    }
    assert!(
        world.entities[&body].position != at(9.0, 2.0),
        "the unit walked"
    );
}
