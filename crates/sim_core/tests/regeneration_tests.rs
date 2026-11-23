//! Regeneration of life and energy against construction, casts and damage (plan
//! `2026-11-23-regeneration`, B-35 and B-36).
//!
//! # Responsibility
//! Checks the regeneration rules `world::upkeep` documents on the public tick API:
//! - No vital regenerates before a structure completes (P-1). A foundation's life follows its
//!   construction curve exactly, damage taken stays taken, and a paused foundation keeps the
//!   life it had when work stopped (P-2).
//! - Life and energy regenerate from the tick after completion (P-3).
//! - Energy regenerates on every tick with no delay: on the tick after a cast and the tick
//!   after damage alike (P-4; no 5.0.14 layer sets `EnergyRegenDelay`, so B-35 is no
//!   deviation).
//! - An uncompleted entity cannot cast, so nothing spends a foundation's energy.
//!
//! # Invariants
//! Archetypes are found by property (`hp_regen_per_tick > 0`, an ability with an energy
//! cost), never by slug, and every rate, delay, life and build time is read from the catalog.
//! Setup may place entities, set a bank and apply damage through `Entity::apply_damage`; the
//! behaviour under test is reached through commands and ticks.

use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceKind, ResourceNode};
use sim_types::{
    AbilityTargetKind, EntityId, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimAngle,
    SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};
use std::error::Error;

const ME: PlayerId = PlayerId(0);

/// The cell at whose minimum corner every foundation of these tests is laid.
const SITE: (u32, u32) = (20, 20);

type TestResult = Result<(), Box<dyn Error>>;

/// The centre of cell (`x`, `y`) in world units.
fn at(x: u32, y: u32) -> SimVec2 {
    SimVec2::new(
        SimScalar::from_num(x) + SimScalar::HALF,
        SimScalar::from_num(y) + SimScalar::HALF,
    )
}

/// One incomplete entity's vitals and construction record before a tick.
#[derive(Debug, Clone, Copy)]
struct FoundationVitals {
    id: EntityId,
    hp: SimScalar,
    energy: SimScalar,
    remaining_ticks: u32,
    hp_per_tick: SimScalar,
}

/// The vitals of every living foundation of `world`.
fn foundation_vitals(world: &SimWorld) -> Vec<FoundationVitals> {
    world
        .entities
        .values()
        .filter(|e| e.is_alive() && !e.is_completed)
        .filter_map(|e| {
            e.construction.map(|record| FoundationVitals {
                id: e.id,
                hp: e.hp,
                energy: e.energy,
                remaining_ticks: record.remaining_ticks,
                hp_per_tick: record.hp_per_tick,
            })
        })
        .collect()
}

/// P-1 over one tick in which nothing damaged a foundation: every foundation still incomplete
/// after the tick gained exactly the construction curve's life for the ticks worked on it, kept
/// its energy, and has no shields.
fn assert_no_regeneration_before_completion(before: &[FoundationVitals], after: &SimWorld) {
    for old in before {
        let Some(entity) = after.entities.get(&old.id) else {
            continue;
        };
        if entity.is_completed {
            continue;
        }
        let remaining = entity
            .construction
            .map_or(0, |record| record.remaining_ticks);
        let worked = old.remaining_ticks - remaining;
        let gained = old.hp_per_tick * SimScalar::from_num(worked);
        assert_eq!(
            entity.hp,
            (old.hp + gained).min(entity.max_hp),
            "foundation {:?} on tick {}: life moves only with the work done",
            old.id,
            after.current_tick
        );
        assert_eq!(
            entity.energy, old.energy,
            "foundation {:?} on tick {}: energy holds until completion",
            old.id, after.current_tick
        );
        assert_eq!(
            entity.shields,
            SimScalar::ZERO,
            "a foundation has no shields"
        );
    }
}

/// Whether a tick checks P-1 after it steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    /// Every foundation follows its construction curve and keeps its energy.
    NoRegeneration,
    /// Only diagnostics are checked: the test pins a rule that holds before and after D-1.
    DiagnosticsOnly,
}

/// Steps `world` one tick with `commands`, checking that the tick raised no diagnostic and,
/// under [`Check::NoRegeneration`], P-1.
fn tick_with(world: &mut SimWorld, commands: Vec<PlayerCommand>, check: Check) {
    let before = foundation_vitals(world);
    let tick = world.current_tick;
    world.step(&TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
    if check == Check::NoRegeneration {
        assert_no_regeneration_before_completion(&before, world);
    }
}

/// Steps `world` `ticks` ticks with no command, checking P-1.
fn idle(world: &mut SimWorld, ticks: u32) {
    idle_with(world, ticks, Check::NoRegeneration);
}

/// Steps `world` `ticks` ticks with no command under `check`.
fn idle_with(world: &mut SimWorld, ticks: u32, check: Check) {
    for _ in 0..ticks {
        tick_with(world, Vec::new(), check);
    }
}

/// Gives `order` to `entities` of `ME` on the next tick.
fn order(world: &mut SimWorld, entities: Vec<EntityId>, order: UnitOrder, check: Check) {
    tick_with(
        world,
        vec![PlayerCommand {
            player_id: ME,
            entities,
            order,
            queue: false,
        }],
        check,
    );
}

/// Every shipped structure that regenerates life.
fn regenerating_structures(world: &SimWorld) -> Vec<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .filter(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .filter(|a| a.hp_regen_per_tick > SimScalar::ZERO)
        .map(|a| a.unit_type_id)
        .collect()
}

/// A flat 64 by 64 world in which `ME` can lay `structure` at [`SITE`]: a builder of its
/// faction, its prerequisite structures completed, money, and a gas node under the site if
/// the structure is an extractor.
fn foundation_world(structure: UnitTypeId) -> Result<(SimWorld, EntityId), Box<dyn Error>> {
    let shipped = sim_fixtures::world(flat_map(Vec::new())?, 7);
    let arch = shipped.catalog.get(structure)?.clone();
    let (width, height) = arch
        .shape
        .footprint()
        .ok_or("a structure has a footprint")?;
    let nodes = if arch.capabilities.is_gas_extractor {
        let cell = (
            SITE.0 + u32::from(width) / 2,
            SITE.1 + u32::from(height) / 2,
        );
        vec![ResourceNode {
            pos: at(cell.0, cell.1),
            kind: ResourceKind::Gas,
            remaining: 500,
            cell,
            placed: None,
        }]
    } else {
        Vec::new()
    };
    let mut world = sim_fixtures::world(flat_map(nodes)?, 7);
    for (slot, required) in (0_u32..).zip(&arch.requires_structures) {
        let id = world
            .catalog
            .id_by_slug(required)
            .ok_or("a prerequisite is in the catalog")?;
        world.spawn_structure(id, ME, 2 + 8 * slot, 50)?;
    }
    let builder = builder_for(&world, &arch)?;
    // Beside the site, so that its sight covers every cell of the largest footprint: a site
    // the player does not see is refused for structures that exclude fogged cells.
    let worker = world.spawn_unit(builder, ME, at(SITE.0 - 2, SITE.1 + 1))?;
    world.player_resources[PlayerSlot::ALL[0]].minerals = 10_000;
    world.player_resources[PlayerSlot::ALL[0]].gas = 10_000;
    idle(&mut world, 1);
    Ok((world, worker))
}

fn flat_map(nodes: Vec<ResourceNode>) -> Result<GameMap, Box<dyn Error>> {
    Ok(GameMap::from_cells(
        64,
        64,
        vec![MapCell::default_ground(CliffLevel(0)); 64 * 64],
        nodes,
        Vec::new(),
    )?)
}

/// The first unit that can raise `structure`.
fn builder_for(world: &SimWorld, structure: &UnitArchetype) -> Result<UnitTypeId, Box<dyn Error>> {
    Ok(world
        .catalog
        .archetypes()
        .find(|a| a.can_raise(structure))
        .map(|a| a.unit_type_id)
        .ok_or("no builder can raise the structure")?)
}

/// Orders `worker` to build `structure` at [`SITE`] and steps until the foundation stands.
fn lay(
    world: &mut SimWorld,
    worker: EntityId,
    structure: UnitTypeId,
    check: Check,
) -> Result<EntityId, Box<dyn Error>> {
    order(
        world,
        vec![worker],
        UnitOrder::Build {
            structure_type: structure,
            cell_x: SITE.0,
            cell_y: SITE.1,
        },
        check,
    );
    if let Some(outcome) = world.game_command_outcomes().first() {
        if let Err(refusal) = &outcome.result {
            return Err(format!("{structure:?}: the build order was refused: {refusal:?}").into());
        }
    }
    for _ in 0..600 {
        if let Some(found) = world
            .entities
            .values()
            .find(|e| e.unit_type_id == structure && !e.is_completed)
        {
            return Ok(found.id);
        }
        idle_with(world, 1, check);
    }
    Err(format!("{structure:?}: the builder never laid the foundation").into())
}

/// Steps until `site` completes, returning its life on each tick before completion and on the
/// completion tick.
fn work_to_completion(
    world: &mut SimWorld,
    site: EntityId,
    check: Check,
) -> Result<(Vec<SimScalar>, SimScalar), Box<dyn Error>> {
    let total = world
        .catalog
        .get(world.entities[&site].unit_type_id)?
        .build_time_ticks;
    let mut before = Vec::new();
    for _ in 0..=total {
        idle_with(world, 1, check);
        let entity = world.entities.get(&site).ok_or("the foundation stands")?;
        if entity.is_completed {
            return Ok((before, entity.hp));
        }
        before.push(entity.hp);
    }
    Err("the foundation never completed".into())
}

/// Damage that leaves a foundation alive and short of full at completion even with the old
/// regeneration under construction: a twentieth of its maximum.
fn small_damage(world: &SimWorld, site: EntityId) -> SimScalar {
    world.entities[&site].max_hp / SimScalar::from_num(20)
}

fn damage(world: &mut SimWorld, id: EntityId, life: SimScalar) -> TestResult {
    world
        .entities
        .get_mut(&id)
        .ok_or("the entity stands")?
        .apply_damage(SimScalar::ZERO, life, SimAngle::ZERO);
    Ok(())
}

#[test]
fn a_foundation_does_not_regenerate_life() -> TestResult {
    let structures = regenerating_structures(&sim_fixtures::world(flat_map(Vec::new())?, 7));
    assert!(
        !structures.is_empty(),
        "the catalog has regenerating structures"
    );
    for structure in structures {
        let (mut world, worker) = foundation_world(structure)?;
        let site = lay(&mut world, worker, structure, Check::NoRegeneration)?;
        idle(&mut world, 10);
        order(
            &mut world,
            vec![worker],
            UnitOrder::Move { target: at(50, 21) },
            Check::NoRegeneration,
        );
        idle(&mut world, 2);
        let record = world.entities[&site]
            .construction
            .ok_or("still a foundation")?;
        let paused = (world.entities[&site].hp, record.remaining_ticks);
        assert!(paused.0 < world.entities[&site].max_hp);
        idle(&mut world, 100);
        let entity = &world.entities[&site];
        let remaining = entity
            .construction
            .ok_or("still a foundation")?
            .remaining_ticks;
        assert_eq!(
            remaining, paused.1,
            "{structure:?}: nobody works the foundation"
        );
        assert_eq!(
            entity.hp, paused.0,
            "{structure:?}: a paused foundation keeps its life"
        );
    }
    Ok(())
}

#[test]
fn a_foundation_completes_on_the_construction_curve() -> TestResult {
    for structure in regenerating_structures(&sim_fixtures::world(flat_map(Vec::new())?, 7)) {
        let (mut world, worker) = foundation_world(structure)?;
        let site = lay(&mut world, worker, structure, Check::NoRegeneration)?;
        let max_hp = world.entities[&site].max_hp;
        let laid_hp = world.entities[&site].hp;
        let (before, at_completion) = work_to_completion(&mut world, site, Check::NoRegeneration)?;
        // The evidence's per-row life at completion (plan §7), printed with --nocapture.
        println!(
            "{structure:?}: laid {laid_hp}, completed {at_completion} of {max_hp} after {} ticks, \
             full on {} ticks before completion",
            before.len(),
            before.iter().filter(|&&hp| hp >= max_hp).count()
        );
        assert!(
            before.iter().all(|&hp| hp < max_hp),
            "{structure:?}: below full on every tick before completion"
        );
        assert_eq!(
            at_completion, max_hp,
            "{structure:?}: full on its completion tick"
        );
    }
    Ok(())
}

#[test]
fn damage_to_a_foundation_stays_taken() -> TestResult {
    for structure in regenerating_structures(&sim_fixtures::world(flat_map(Vec::new())?, 7)) {
        let (mut world, worker) = foundation_world(structure)?;
        let site = lay(&mut world, worker, structure, Check::NoRegeneration)?;
        let total = world.catalog.get(structure)?.build_time_ticks;
        idle(&mut world, total / 2);
        let hit = small_damage(&world, site);
        damage(&mut world, site, hit)?;
        let (_, at_completion) = work_to_completion(&mut world, site, Check::NoRegeneration)?;
        assert_eq!(
            at_completion,
            world.entities[&site].max_hp - hit,
            "{structure:?}: completes short by exactly the damage taken"
        );
    }
    Ok(())
}

#[test]
fn a_completed_structure_regenerates_from_the_next_tick() -> TestResult {
    for structure in regenerating_structures(&sim_fixtures::world(flat_map(Vec::new())?, 7)) {
        let (mut world, worker) = foundation_world(structure)?;
        let site = lay(&mut world, worker, structure, Check::DiagnosticsOnly)?;
        let total = world.catalog.get(structure)?.build_time_ticks;
        idle_with(&mut world, total / 2, Check::DiagnosticsOnly);
        let hit = small_damage(&world, site);
        damage(&mut world, site, hit)?;
        let (_, completed_hp) = work_to_completion(&mut world, site, Check::DiagnosticsOnly)?;
        let completed_energy = world.entities[&site].energy;
        idle_with(&mut world, 1, Check::DiagnosticsOnly);
        let arch = world.catalog.get(structure)?;
        let entity = &world.entities[&site];
        let life_gain = if entity.ticks_since_last_damage >= arch.hp_regen_delay_ticks {
            arch.hp_regen_per_tick.min(entity.max_hp - completed_hp)
        } else {
            SimScalar::ZERO
        };
        assert!(
            life_gain > SimScalar::ZERO,
            "{structure:?}: the hit was long ago"
        );
        assert_eq!(entity.hp, completed_hp + life_gain, "{structure:?}: life");
        let energy_gain = arch
            .energy_regen_per_tick
            .min(arch.max_energy - completed_energy);
        assert_eq!(
            entity.energy,
            completed_energy + energy_gain,
            "{structure:?}: energy"
        );
    }
    Ok(())
}

#[test]
fn a_foundation_holds_its_start_energy() -> TestResult {
    let probe = sim_fixtures::world(flat_map(Vec::new())?, 7);
    let builder = probe
        .catalog
        .archetypes()
        .find(|a| a.capabilities.can_construct)
        .ok_or("the catalog has a builder")?;
    // A scratch structure with energy: a shipped structure the builder raises with no
    // prerequisite and no gas node, given a maximum of 100, a start of 50 and a rate of 1.
    let structure = probe
        .catalog
        .archetypes()
        .find(|a| {
            builder.can_raise(a)
                && a.requires_structures.is_empty()
                && !a.capabilities.is_gas_extractor
        })
        .map(|a| a.unit_type_id)
        .ok_or("the builder raises a structure without prerequisites")?;
    let (mut world, worker) = foundation_world(structure)?;
    let scratch = world.catalog.get_mut(structure)?;
    scratch.max_energy = SimScalar::from_num(100);
    scratch.start_energy = SimScalar::from_num(50);
    scratch.energy_regen_per_tick = SimScalar::ONE;
    let site = lay(&mut world, worker, structure, Check::NoRegeneration)?;
    assert_eq!(world.entities[&site].energy, SimScalar::from_num(50));
    work_to_completion(&mut world, site, Check::NoRegeneration)?;
    assert_eq!(
        world.entities[&site].energy,
        SimScalar::from_num(50),
        "no energy gained before or on the completion tick"
    );
    idle(&mut world, 1);
    assert_eq!(world.entities[&site].energy, SimScalar::from_num(51));
    Ok(())
}

/// A shipped caster with an energy-costing point ability it may cast now at a point beside it:
/// the world, the caster, the ability's index and the point.
fn caster_world() -> Result<(SimWorld, EntityId, u8, SimVec2), Box<dyn Error>> {
    let mut world = sim_fixtures::world(flat_map(Vec::new())?, 7);
    let candidates: Vec<(UnitTypeId, u8, SimScalar)> = world
        .catalog
        .archetypes()
        .filter(|a| a.energy_regen_per_tick > SimScalar::ZERO)
        .flat_map(|a| {
            a.abilities
                .iter()
                .zip(0_u8..)
                .filter(|(ability, _)| {
                    ability.energy_cost > SimScalar::ZERO
                        && ability.target_kind == AbilityTargetKind::TargetPosition
                })
                .map(|(ability, index)| (a.unit_type_id, index, ability.energy_cost))
        })
        .collect();
    let point = at(33, 32);
    for (unit_type, index, cost) in candidates {
        let caster = world.spawn_unit(unit_type, ME, at(30, 32))?;
        // Enough for one cast, far enough below the maximum that the rate shows.
        world.entities.get_mut(&caster).ok_or("caster")?.energy = cost + SimScalar::from_num(10);
        if world.would_accept_cast_order(caster, index, Some(point), None) {
            return Ok((world, caster, index, point));
        }
        world.entities.get_mut(&caster).ok_or("caster")?.hp = SimScalar::ZERO;
        idle(&mut world, 1);
    }
    Err("no shipped caster may cast a point ability without research".into())
}

#[test]
fn energy_regenerates_on_the_tick_after_a_cast() -> TestResult {
    let (mut world, caster, index, point) = caster_world()?;
    let arch = world
        .catalog
        .get(world.entities[&caster].unit_type_id)?
        .clone();
    let cost = arch.abilities[usize::from(index)].energy_cost;
    let rate = arch.energy_regen_per_tick;
    let mut previous = world.entities[&caster].energy;
    order(
        &mut world,
        vec![caster],
        UnitOrder::CastAbility {
            ability_index: index,
            target_pos: Some(point),
            target_entity: None,
        },
        Check::NoRegeneration,
    );
    for _ in 0..50 {
        let now = world.entities[&caster].energy;
        if now < previous {
            assert_eq!(now, previous + rate - cost, "the cast tick regenerates too");
            idle(&mut world, 1);
            assert_eq!(
                world.entities[&caster].energy,
                now + rate,
                "the tick after a cast regenerates in full"
            );
            return Ok(());
        }
        assert_eq!(
            now,
            previous + rate,
            "every tick before the cast regenerates"
        );
        previous = now;
        idle(&mut world, 1);
    }
    Err("the caster never cast".into())
}

#[test]
fn energy_regenerates_on_the_tick_after_damage() -> TestResult {
    let (mut world, caster, _, _) = caster_world()?;
    let rate = world
        .catalog
        .get(world.entities[&caster].unit_type_id)?
        .energy_regen_per_tick;
    let energy = world.entities[&caster].energy;
    damage(&mut world, caster, SimScalar::ONE)?;
    idle(&mut world, 1);
    assert_eq!(world.entities[&caster].ticks_since_last_damage, 1);
    assert_eq!(world.entities[&caster].energy, energy + rate);
    damage(&mut world, caster, SimScalar::ONE)?;
    idle(&mut world, 1);
    assert_eq!(world.entities[&caster].energy, energy + rate + rate);
    Ok(())
}

#[test]
fn an_uncompleted_entity_cannot_cast() -> TestResult {
    let (mut world, caster, index, point) = caster_world()?;
    assert!(world.would_admit_cast(caster, index, Some(point), None));
    world
        .entities
        .get_mut(&caster)
        .ok_or("caster")?
        .is_completed = false;
    assert!(!world.would_accept_cast_order(caster, index, Some(point), None));
    assert!(!world.would_admit_cast(caster, index, Some(point), None));
    Ok(())
}
