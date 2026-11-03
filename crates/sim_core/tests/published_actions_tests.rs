//! The publishable-actions rule, held against what real worlds put in their snapshots.
//!
//! # Responsibility
//! `UnitCatalog::publishable_actions` decides which media a presentation layer must hold. If it
//! left out a state the simulation can publish, a client trusting it would stop a match with a
//! media failure. This file plays worlds and fails if any snapshot shows a state the rule omits.
//!
//! # How it fits
//! The rule is read from catalog data in `sim_core/src/published_actions.rs`. Here every
//! archetype of the shipped catalog fights, moves and casts every ability it declares, on open
//! ground. `full_match_tests.rs` holds the same rule over a whole scripted match, which adds
//! harvesting and construction.
//!
//! # Invariants
//! - Only public commands drive the world after setup.
//! - The check is containment: observed states are a subset of the rule's states. The rule may
//!   over-approximate, and a separate assertion makes sure the run is not vacuous.

use sim_core::UnitCatalog;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AnimState, PlayerCommand, PlayerId, RenderSnapshot, ShapeKind, SimScalar, SimVec2, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};
use std::collections::BTreeSet;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

/// Side length of the open test map, in cells.
const MAP_SIZE: u32 = 96;
/// Ticks the melee runs: long enough for every unit to close, fight and finish casting.
const MELEE_TICKS: u32 = 600;
/// Ticks at which every unit is told to use each ability it declares.
const CAST_TICKS: [u32; 2] = [20, 300];

fn command(
    player: PlayerId,
    entities: Vec<sim_types::EntityId>,
    order: UnitOrder,
) -> PlayerCommand {
    PlayerCommand {
        player_id: player,
        entities,
        order,
        queue: false,
    }
}

fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y))
}

/// Records every `(identity, state)` the snapshot shows.
fn observe(snapshot: &RenderSnapshot, seen: &mut BTreeSet<(UnitTypeId, AnimState)>) {
    seen.extend(
        snapshot
            .bubbles
            .iter()
            .map(|bubble| (bubble.unit_type, bubble.anim_state)),
    );
}

/// Asserts every observation is allowed by the rule, and returns the states seen at all.
fn assert_allowed(
    catalog: &UnitCatalog,
    seen: &BTreeSet<(UnitTypeId, AnimState)>,
) -> Result<BTreeSet<AnimState>, Box<dyn Error>> {
    let mut states = BTreeSet::new();
    for &(unit_type, state) in seen {
        let archetype = catalog.get(unit_type)?;
        assert!(
            catalog.publishable_actions(archetype).contains(state),
            "{} published {state:?}, which the rule leaves out",
            archetype.slug
        );
        states.insert(state);
    }
    Ok(states)
}

#[test]
fn every_archetype_fighting_and_casting_publishes_only_what_the_rule_allows() -> TestResult {
    let map = GameMap::new(MAP_SIZE, MAP_SIZE, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, 11);
    let catalog = world.catalog.clone();
    let sides = [(PlayerId(0), 30), (PlayerId(1), 66)];
    let mut armies: [Vec<sim_types::EntityId>; 2] = [Vec::new(), Vec::new()];
    for (side, &(owner, row)) in sides.iter().enumerate() {
        let mut column = 4;
        let mut structure_column = 2_u32;
        for archetype in catalog.archetypes() {
            if let ShapeKind::BuildingDome { footprint_w, .. } = archetype.shape {
                let cell_y = if side == 0 { 2 } else { MAP_SIZE - 12 };
                if structure_column + u32::from(footprint_w) < MAP_SIZE
                    && world
                        .spawn_structure(archetype.unit_type_id, owner, structure_column, cell_y)
                        .is_ok()
                {
                    structure_column += u32::from(footprint_w) + 1;
                }
            } else {
                armies[side].push(world.spawn_unit(
                    archetype.unit_type_id,
                    owner,
                    at(column, row),
                )?);
                column += 3;
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut snapshot = RenderSnapshot::default();
    for tick in 0..MELEE_TICKS {
        let mut commands = Vec::new();
        if tick == 0 {
            for (side, &(owner, _)) in sides.iter().enumerate() {
                let enemy_row = sides[1 - side].1;
                commands.push(command(
                    owner,
                    armies[side].clone(),
                    UnitOrder::AttackMove {
                        target: at(48, enemy_row),
                    },
                ));
            }
        }
        if CAST_TICKS.contains(&tick) {
            for (side, &(owner, _)) in sides.iter().enumerate() {
                for &id in &armies[side] {
                    let Some(entity) = world.entities.get(&id) else {
                        continue;
                    };
                    let abilities = catalog.get(entity.unit_type_id)?.abilities.len();
                    for index in 0..abilities {
                        commands.push(command(
                            owner,
                            vec![id],
                            UnitOrder::CastAbility {
                                ability_index: u8::try_from(index)?,
                                target_pos: Some(entity.position),
                                target_entity: None,
                            },
                        ));
                    }
                }
            }
        }
        world.step_into(
            &TickBundle {
                tick: Tick(tick),
                commands,
                state_checksum: None,
            },
            &mut snapshot,
        );
        observe(&snapshot, &mut seen);
    }
    let states = assert_allowed(&catalog, &seen)?;
    for state in [AnimState::Idle, AnimState::Walk, AnimState::Attack] {
        assert!(states.contains(&state), "the melee never showed {state:?}");
    }
    let structures_seen = seen.iter().any(|&(unit_type, _)| {
        catalog
            .get(unit_type)
            .is_ok_and(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
    });
    assert!(structures_seen, "structures must take part");
    Ok(())
}
