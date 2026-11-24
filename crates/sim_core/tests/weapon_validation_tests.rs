//! Missiles validate their target in flight, and weapons strike only what their player sees.
//!
//! # Responsibility
//! Pins plan `2026-11-24-weapon-validation` against `StarCraft` II 5.0.14:
//! - D-2 (P-3): a teleport is counted on the unit it relocates; nothing else counts one.
//!
//! # How it fits
//! The teleport count (`Entity::teleport_count`) is written by `Entity::teleport_to`, which every
//! teleport effect goes through; projectiles record it at launch (`ActiveProjectile::
//! target_teleport_count`), and the combat phase compares the two in flight.
//!
//! # Invariants
//! - No unit is named: archetypes and weapons are found by their properties, and Blink is
//!   authored on a scratch copy of a shipped archetype, since no shipped unit carries it
//!   (charter §3.3).
//! - Every world is flat unless a test says otherwise.

use sim_core::SimWorld;
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, MoverType, PlayerId, SimScalar,
    SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn point(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn bundle(tick: u32) -> TickBundle {
    TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    }
}

/// Steps `world` from `first` through `last`, inclusive.
fn steps(world: &mut SimWorld, first: u32, last: u32) {
    for tick in first..=last {
        world.step(&bundle(tick));
    }
}

/// The next free archetype identity.
fn free_type(world: &SimWorld) -> UnitTypeId {
    UnitTypeId(
        world
            .catalog
            .archetypes()
            .map(|arch| arch.unit_type_id.0)
            .max()
            .unwrap_or(0)
            + 1,
    )
}

/// A Blink in the shape of 5.0.14's `Blink` (`CEffectTeleport`, `liberty` EffectData:2347–2356):
/// a point-targeted teleport, free and without cooldown so a test may cast it at will.
fn blink_ability() -> AbilitySpec {
    AbilitySpec {
        name: "scratch blink".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cast_range: SimScalar::from_num(30),
        cooldown_ticks: 0,
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_num(30),
            cooldown_ticks: 0,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }
}

/// Registers an unarmed scratch copy of a shipped mobile ground archetype whose only ability is
/// [`blink_ability`]; returns its identity.
fn register_blinker(world: &mut SimWorld) -> Result<UnitTypeId, String> {
    let mut arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.mover_type == MoverType::Ground && arch.max_speed > SimScalar::ZERO)
        .cloned()
        .ok_or("the shipped catalog has a mobile ground archetype")?;
    arch.unit_type_id = free_type(world);
    "scratch_blinker".clone_into(&mut arch.slug);
    arch.weapon = None;
    arch.ground_weapon = None;
    arch.air_weapon = None;
    arch.abilities = vec![blink_ability()];
    let id = arch.unit_type_id;
    world.catalog.register(arch);
    Ok(id)
}

/// A flat 32x32 world with one blinker of player 1 at (8, 8).
fn blink_world(seed: u64) -> Result<(SimWorld, EntityId), Box<dyn std::error::Error>> {
    let map = GameMap::new(32, 32, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = sim_fixtures::world(map, seed);
    let kind = register_blinker(&mut world)?;
    let unit = world.spawn_unit(kind, PlayerId(1), point(8.0, 8.0))?;
    Ok((world, unit))
}

/// P-3: a Blink relocates the unit and counts one teleport.
#[test]
fn a_blink_counts_one_teleport() -> TestResult {
    let (mut world, unit) = blink_world(0xB10)?;
    assert_eq!(world.entities[&unit].teleport_count, 0);
    let destination = point(14.0, 8.0);
    assert!(
        world.execute_ability(unit, 0, Some(destination), None),
        "the blink is admitted"
    );
    assert_eq!(world.entities[&unit].position, destination);
    assert_eq!(world.entities[&unit].teleport_count, 1);
    assert!(world.execute_ability(unit, 0, Some(point(14.0, 14.0)), None));
    assert_eq!(world.entities[&unit].teleport_count, 2, "one per Blink");
    Ok(())
}

/// P-3: walking, boarding and leaving a transport relocate a unit without a teleport.
#[test]
fn walking_and_an_unload_count_no_teleport() -> TestResult {
    let (mut world, unit) = blink_world(0xB11)?;
    let kind = world.entities[&unit].unit_type_id;
    world
        .entities
        .get_mut(&unit)
        .ok_or("the unit exists")?
        .issue_order(
            UnitOrder::Move {
                target: point(20.0, 8.0),
            },
            false,
        )
        .map_err(|error| format!("{error:?}"))?;
    steps(&mut world, 1, 40);
    assert!(
        world.entities[&unit].position.x > SimScalar::from_num(9),
        "the unit walked"
    );
    assert_eq!(
        world.entities[&unit].teleport_count, 0,
        "a walk is no teleport"
    );
    let carrier = world.spawn_unit(kind, PlayerId(1), point(24.0, 24.0))?;
    world
        .try_load_unit_into_transport(unit, carrier)
        .map_err(|error| format!("{error:?}"))?;
    world
        .try_unload_unit_from_transport(unit, point(4.0, 24.0))
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(world.entities[&unit].position, point(4.0, 24.0));
    assert_eq!(
        world.entities[&unit].teleport_count, 0,
        "an unload is no teleport"
    );
    Ok(())
}

/// Casts every unit-targeted effect ability of every shipped archetype on the blinker, one fresh
/// world each, until one flies it (`Entity::displacement`, a `Displace` node); returns the
/// world once the flight has ended and the blinker, or `None` if no shipped ability displaces.
fn displaced_blinker() -> Result<Option<(SimWorld, EntityId)>, Box<dyn std::error::Error>> {
    let (probe, _) = blink_world(0xB12)?;
    let casts: Vec<(UnitTypeId, u8)> = probe
        .catalog
        .archetypes()
        .flat_map(|arch| {
            arch.abilities
                .iter()
                .enumerate()
                .filter(|(_, ability)| {
                    ability.target_kind == AbilityTargetKind::TargetEntity
                        && matches!(ability.effect, AbilityEffectSpec::Effect(_))
                })
                .filter_map(|(index, _)| u8::try_from(index).ok())
                .map(|index| (arch.unit_type_id, index))
                .collect::<Vec<_>>()
        })
        .collect();
    for (kind, index) in casts {
        let (mut world, unit) = blink_world(0xB12)?;
        let caster = world.spawn_unit(kind, PlayerId(0), point(11.0, 8.0))?;
        if let Some(entity) = world.entities.get_mut(&caster) {
            entity.energy = SimScalar::from_num(200);
        }
        // One tick first, so the caster's player sees its target (abilities may require it).
        world.step(&bundle(1));
        if !world.execute_ability(caster, index, None, Some(unit)) {
            continue;
        }
        let mut flown = false;
        for tick in 2..=120 {
            world.step(&bundle(tick));
            let in_flight = world
                .entities
                .get(&unit)
                .is_some_and(|entity| entity.displacement.is_some());
            flown |= in_flight;
            if flown && !in_flight {
                return Ok(Some((world, unit)));
            }
        }
    }
    Ok(None)
}

/// P-3: a `Displace` flight (a pull, 5.0.14's Abduct is a launch, not a `CEffectTeleport`) moves
/// a unit without a teleport.
#[test]
fn a_displacement_counts_no_teleport() -> TestResult {
    let (world, unit) = displaced_blinker()?.ok_or("a shipped ability displaces its target")?;
    let entity = world.entities.get(&unit).ok_or("the flown unit lives")?;
    assert_ne!(entity.position, point(8.0, 8.0), "the unit was flown");
    assert_eq!(entity.teleport_count, 0, "a displacement is no teleport");
    Ok(())
}
