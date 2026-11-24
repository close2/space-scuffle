//! # Integration Test: the plain orders of a unit's settled stance
//!
//! Plan `2026-11-23-anchored-commands` WP1 (D-1, D-2; acceptance P-2). Each mode in
//! `units.ron` authors which of Stop and Hold Position a unit in it accepts
//! (`UnitModeSpec::orders`, a [`StanceOrders`]); the archetype's own stance accepts Stop when
//! the archetype is mobile or armed and Hold Position when it is mobile (plan
//! `2026-11-23-stance-checked-orders` D-2, B-171). [`UnitArchetype::stance_orders`] is that
//! rule, and [`sim_core::entity::Entity::settled_stance_orders`] applies it to the stance a unit
//! is in or, while it channels a switch, the stance it will be in when the channel ends. The
//! client's command card and its admission both read it, so a button is shown exactly when
//! pressing it reaches a unit. The simulation applies it too: `SimWorld::issue_plain_order`
//! gives Stop and Hold Position only to the named units whose settled stance takes them (plan
//! `2026-11-23-stance-checked-orders` D-1, B-172), and [`UnitArchetype::stance_armed`] and
//! `Entity::settled_armed` say whether a stance carries a weapon (D-3).
//!
//! Every test drives the shipped catalog through `SimWorld::spawn_unit`,
//! `SimWorld::execute_ability` and `SimWorld::step`. Archetypes and modes are found by property;
//! no slug or mode name is written here (charter C5).

#![allow(clippy::expect_used, clippy::panic)]

use sim_core::catalog::RuntimeCatalogs;
use sim_core::{SimWorld, UnitArchetype};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, EntityId, MoverType, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2,
    StanceOrders, Tick, TickBundle, UnitOrder,
};

/// Ticks a switch may take at most, its random extra included, before a test gives up.
const PATIENCE: u32 = 400;

/// A 48 × 48 flat world on the shipped catalog.
fn flat_world(seed: u64) -> SimWorld {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat 48x48 map is valid");
    sim_fixtures::world(map, seed)
}

/// Steps `world` one tick without commands.
fn step(world: &mut SimWorld) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands: Vec::new(),
        state_checksum: None,
    });
}

/// Every shipped archetype that declares a mode, with each mode's switch ability index.
fn moded_archetypes(world: &SimWorld) -> Vec<(UnitArchetype, Vec<(usize, u8)>)> {
    world
        .catalog
        .archetypes()
        .filter(|arch| !arch.modes.is_empty())
        .map(|arch| {
            let switches = arch
                .modes
                .iter()
                .enumerate()
                .map(|(mode, spec)| {
                    let ability = arch
                        .abilities
                        .iter()
                        .position(|ability| {
                            matches!(&ability.effect,
                                AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == spec.mode_name)
                        })
                        .expect("every shipped mode has a switch ability");
                    (mode, u8::try_from(ability).expect("few abilities"))
                })
                .collect();
            (arch.clone(), switches)
        })
        .collect()
}

/// The settled stance's plain orders of `unit`.
fn settled_orders(world: &SimWorld, unit: EntityId) -> StanceOrders {
    let entity = &world.entities[&unit];
    let archetype = world
        .catalog
        .get(entity.unit_type_id)
        .expect("the unit's archetype");
    entity.settled_stance_orders(archetype)
}

/// Steps `world` until `unit` no longer channels.
fn finish_channel(world: &mut SimWorld, unit: EntityId) {
    for _ in 0..PATIENCE {
        if world.entities[&unit].channeling_remaining_ticks == 0 {
            return;
        }
        step(world);
    }
    panic!("the switch ends within {PATIENCE} ticks");
}

#[test]
fn a_mode_accepts_exactly_its_authored_orders() {
    let world = flat_world(0x5a_0002);
    let moded = moded_archetypes(&world);
    assert!(!moded.is_empty(), "the shipped catalog has a mode");
    for (arch, _) in &moded {
        for mode in &arch.modes {
            assert_eq!(
                arch.stance_orders(Some(mode)),
                mode.orders,
                "{} {}",
                arch.slug,
                mode.mode_name
            );
        }
    }
}

#[test]
fn a_unit_answers_for_the_stance_it_settles_in_through_both_channels() {
    for (arch, switches) in moded_archetypes(&flat_world(0x5a_0003)) {
        for (mode, ability) in switches {
            let mut world = flat_world(0x5a_0003);
            let unit = world
                .spawn_unit(
                    arch.unit_type_id,
                    PlayerId(0),
                    SimVec2::new(SimScalar::from_num(20), SimScalar::from_num(20)),
                )
                .expect("spawn the moded unit");
            let label = format!("{} {}", arch.slug, arch.modes[mode].mode_name);
            let default = arch.stance_orders(None);
            let authored = arch.modes[mode].orders;
            assert_eq!(settled_orders(&world, unit), default, "{label}: default");

            assert!(world.execute_ability(unit, ability, None, None), "{label}");
            assert!(world.entities[&unit].channeling_remaining_ticks > 0);
            assert_eq!(world.entities[&unit].active_mode, None, "{label}");
            assert_eq!(
                settled_orders(&world, unit),
                authored,
                "{label}: entering answers for the mode"
            );
            finish_channel(&mut world, unit);
            assert_eq!(
                settled_orders(&world, unit),
                authored,
                "{label}: in the mode"
            );

            assert!(world.execute_ability(unit, ability, None, None), "{label}");
            assert!(world.entities[&unit].channeling_remaining_ticks > 0);
            assert!(world.entities[&unit].active_mode.is_some(), "{label}");
            assert_eq!(
                settled_orders(&world, unit),
                default,
                "{label}: leaving answers for the default stance"
            );
            finish_channel(&mut world, unit);
            assert_eq!(settled_orders(&world, unit), default, "{label}: back");
        }
    }
}

#[test]
fn the_shipped_modes_differ_from_their_default_stance_in_hold_position_only_where_authored() {
    // The shipped anchoring modes all keep Stop (5.0.14 `SiegeTankSieged` and
    // `LurkerMPBurrowed` carry `stop`), and at least one gives up Hold Position (the sieged
    // tank has no `move`): the rule is not the mobility rule.
    let world = flat_world(0x5a_0004);
    let anchoring: Vec<_> = world
        .catalog
        .archetypes()
        .flat_map(|arch| arch.modes.iter())
        .filter(|mode| mode.is_immobile)
        .collect();
    assert!(!anchoring.is_empty(), "a shipped mode anchors");
    assert!(anchoring.iter().all(|mode| mode.orders.stop));
    assert!(anchoring.iter().any(|mode| !mode.orders.hold_position));
    assert!(anchoring.iter().any(|mode| mode.orders.hold_position));
}

// ---------------------------------------------------------------------------------------------
// Plan `2026-11-23-stance-checked-orders` (B-171, B-172): the simulation itself admits Stop and
// Hold Position by the settled stance, and the default stance's Stop is "mobile or armed".
// ---------------------------------------------------------------------------------------------

/// Where a test unit stands in its stances when the command arrives.
#[derive(Debug, Clone, Copy)]
enum Stance {
    /// The archetype's own stance, no channel.
    Default,
    /// Channelling the switch into mode `.0`.
    Entering(usize),
    /// Settled in mode `.0`.
    Settled(usize),
    /// Channelling the switch out of mode `.0`.
    Leaving(usize),
}

/// The owner of every test unit.
const ME: PlayerId = PlayerId(0);

/// A world on `catalogs` with one unit of `arch` owned by [`ME`], brought into `stance`
/// (`switches` as [`moded_archetypes`] lists them). A structure stands on cells (20, 20).
fn unit_in(
    catalogs: &RuntimeCatalogs,
    arch: &UnitArchetype,
    switches: &[(usize, u8)],
    stance: Stance,
) -> Result<(SimWorld, EntityId), Box<dyn std::error::Error>> {
    let map = GameMap::new(48, 48, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::with_catalogs(
        map,
        0x5a_0100,
        catalogs.units.clone(),
        catalogs.upgrades.clone(),
    )?;
    let unit = if matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
        world.spawn_structure(arch.unit_type_id, ME, 20, 20)?
    } else {
        world.spawn_unit(arch.unit_type_id, ME, at(20, 20))?
    };
    let switch_of = |mode: usize| {
        switches
            .iter()
            .find(|(index, _)| *index == mode)
            .map(|(_, ability)| *ability)
            .ok_or("the mode's switch")
    };
    match stance {
        Stance::Default => {}
        Stance::Entering(mode) => {
            assert!(world.execute_ability(unit, switch_of(mode)?, None, None));
        }
        Stance::Settled(mode) | Stance::Leaving(mode) => {
            assert!(world.execute_ability(unit, switch_of(mode)?, None, None));
            finish_channel(&mut world, unit);
            if matches!(stance, Stance::Leaving(_)) {
                assert!(world.execute_ability(unit, switch_of(mode)?, None, None));
            }
        }
    }
    Ok((world, unit))
}

/// A world point at whole world units.
fn at(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

/// Steps `world` one tick with `commands`.
fn step_with(world: &mut SimWorld, commands: Vec<PlayerCommand>) {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands,
        state_checksum: None,
    });
}

/// A command from [`ME`] to `entities`.
fn command(entities: Vec<EntityId>, order: UnitOrder, queue: bool) -> PlayerCommand {
    PlayerCommand {
        player_id: ME,
        entities,
        order,
        queue,
    }
}

/// Every stance a unit of `arch` can be in: the default, and per mode both channels and the
/// settled mode.
fn stances(switches: &[(usize, u8)]) -> Vec<Stance> {
    let mut all = vec![Stance::Default];
    for &(mode, _) in switches {
        all.extend([
            Stance::Entering(mode),
            Stance::Settled(mode),
            Stance::Leaving(mode),
        ]);
    }
    all
}

/// The mode switches of `arch`, as [`moded_archetypes`] lists them; empty without modes.
fn switches_of(arch: &UnitArchetype) -> Vec<(usize, u8)> {
    arch.modes
        .iter()
        .enumerate()
        .filter_map(|(mode, spec)| {
            let ability = arch.abilities.iter().position(|ability| {
                matches!(&ability.effect,
                    AbilityEffectSpec::SwitchMode { mode_name } if *mode_name == spec.mode_name)
            })?;
            Some((mode, u8::try_from(ability).ok()?))
        })
        .collect()
}

/// Whether `arch` stands on cells that a gas extractor needs a geyser for; those are spawned
/// only on one, so they are left out of the exhaustive set and counted.
fn needs_a_geyser(arch: &UnitArchetype) -> bool {
    arch.capabilities.is_gas_extractor
}

/// P-1: for every shipped archetype in each of its stances, a Stop or Hold Position, immediate
/// or queued, reaches the unit exactly when its settled stance takes it. A unit it reaches ends
/// the tick as if the order had been given to it directly; a unit it does not reach ends the
/// tick equal, field for field, to the same unit in a world that received no command.
#[test]
fn the_simulation_gives_stop_and_hold_only_to_a_stance_that_takes_them(
) -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (mut taken, mut refused, mut observable_refusals) = (0, 0, 0);
    for arch in catalogs
        .units
        .archetypes()
        .filter(|arch| !needs_a_geyser(arch))
    {
        let switches = switches_of(arch);
        for stance in stances(&switches) {
            for order in [UnitOrder::Stop, UnitOrder::HoldPosition] {
                for queue in [false, true] {
                    let label = format!("{} {stance:?} {order:?} queue {queue}", arch.slug);
                    let (mut commanded, unit) = unit_in(&catalogs, arch, &switches, stance)?;
                    let (mut untouched, _) = unit_in(&catalogs, arch, &switches, stance)?;
                    let (mut forced, _) = unit_in(&catalogs, arch, &switches, stance)?;
                    let orders = settled_orders(&commanded, unit);
                    let takes = match order {
                        UnitOrder::Stop => orders.stop,
                        _ => orders.hold_position,
                    };
                    forced
                        .entities
                        .get_mut(&unit)
                        .ok_or("the forced unit")?
                        .issue_order(order.clone(), queue)?;
                    step_with(
                        &mut commanded,
                        vec![command(vec![unit], order.clone(), queue)],
                    );
                    step(&mut untouched);
                    step(&mut forced);
                    let expected = if takes { &forced } else { &untouched };
                    assert_eq!(
                        commanded.entities[&unit], expected.entities[&unit],
                        "{label}: takes {takes}"
                    );
                    assert_eq!(
                        commanded.compute_checksum(),
                        expected.compute_checksum(),
                        "{label}: takes {takes}"
                    );
                    if takes {
                        taken += 1;
                    } else {
                        refused += 1;
                        if forced.entities[&unit] != untouched.entities[&unit] {
                            observable_refusals += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(taken > 0 && refused > 0, "{taken} taken, {refused} refused");
    assert!(
        observable_refusals > 0,
        "a refused order would have changed a unit: {observable_refusals}"
    );
    Ok(())
}

/// An archetype, its mode switches (as [`switches_of`] lists them) and the index of one mode.
type Anchored = (UnitArchetype, Vec<(usize, u8)>, usize);

/// An anchoring mode whose stance lacks Hold Position (the sieged tank's), with its archetype
/// and switch, and a mobile archetype armed by default without modes; both found by property.
fn sieged_and_mobile(
    catalogs: &RuntimeCatalogs,
) -> Result<(Anchored, UnitArchetype), Box<dyn std::error::Error>> {
    let anchored = catalogs
        .units
        .archetypes()
        .find_map(|arch| {
            let switches = switches_of(arch);
            let mode = switches.iter().map(|(mode, _)| *mode).find(|&mode| {
                let spec = &arch.modes[mode];
                spec.is_immobile && spec.orders.stop && !spec.orders.hold_position
            })?;
            Some((arch.clone(), switches, mode))
        })
        .ok_or("an anchoring mode with Stop and no Hold Position")?;
    let mobile = catalogs
        .units
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default() && arch.modes.is_empty() && arch.stance_armed(None)
        })
        .ok_or("a mobile armed archetype without modes")?
        .clone();
    Ok((anchored, mobile))
}

/// P-2 (`ToSelection`, 5.0.14 `core` AbilData:1015-1016, :553): one command naming a sieged
/// unit and a mobile one gives Hold Position to the mobile one only and Stop to both; a command
/// every recipient refuses changes nothing.
#[test]
fn one_command_reaches_each_named_unit_whose_stance_takes_it(
) -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let ((tank_arch, switches, mode), mobile_arch) = sieged_and_mobile(&catalogs)?;
    let build = || -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
        let (mut world, tank) = unit_in(&catalogs, &tank_arch, &switches, Stance::Settled(mode))?;
        let walker = world.spawn_unit(mobile_arch.unit_type_id, ME, at(30, 30))?;
        Ok((world, tank, walker))
    };

    let (mut world, tank, walker) = build()?;
    let (mut untouched, ..) = build()?;
    step_with(
        &mut world,
        vec![command(vec![tank, walker], UnitOrder::HoldPosition, false)],
    );
    step(&mut untouched);
    assert!(
        world.entities[&walker].is_holding_position,
        "the walker holds"
    );
    assert_eq!(
        world.entities[&tank], untouched.entities[&tank],
        "the sieged unit is left as it was"
    );

    let (mut world, tank, walker) = build()?;
    step_with(
        &mut world,
        vec![command(vec![tank, walker], UnitOrder::Stop, true)],
    );
    for unit in [tank, walker] {
        assert_eq!(
            world.entities[&unit].order_queue.back(),
            Some(&UnitOrder::Stop),
            "a queued Stop reaches both"
        );
    }

    let (mut world, tank, _) = build()?;
    let (mut untouched, ..) = build()?;
    step_with(
        &mut world,
        vec![command(vec![tank], UnitOrder::HoldPosition, false)],
    );
    step(&mut untouched);
    assert_eq!(
        world.compute_checksum(),
        untouched.compute_checksum(),
        "a command no recipient takes changes nothing"
    );
    Ok(())
}

/// P-4's rule (B-171, plan `2026-11-23-stance-checked-orders` D-2): the default stance takes
/// Stop when the archetype is mobile or armed, and Hold Position when it is mobile.
#[test]
fn the_default_stance_takes_stop_when_mobile_or_armed_and_hold_when_mobile() {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (mut armed_structures, mut unarmed_structures) = (0, 0);
    for arch in catalogs.units.archetypes() {
        let mobile = !arch.is_immobile_by_default();
        let armed = arch.stance_armed(None);
        // The A command has its own derivation and table (plan `2026-11-24-attack-orders` P-1).
        let orders = arch.stance_orders(None);
        assert_eq!(
            (orders.stop, orders.hold_position),
            (mobile || armed, mobile),
            "{}",
            arch.slug
        );
        if !mobile {
            if armed {
                armed_structures += 1;
            } else {
                unarmed_structures += 1;
            }
        }
    }
    assert!(
        armed_structures > 0 && unarmed_structures > 0,
        "{armed_structures} armed and {unarmed_structures} unarmed immobile archetypes"
    );
}

/// P-6: `stance_armed` is true exactly when a unit spawned and switched into that stance has a
/// weapon against ground or air, as the simulation picks it (`weapon_against` with the unit's
/// active replacement weapon).
#[test]
fn armed_in_a_stance_means_the_live_unit_has_a_weapon() -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (mut armed, mut unarmed) = (0, 0);
    for arch in catalogs
        .units
        .archetypes()
        .filter(|arch| !needs_a_geyser(arch))
    {
        let switches = switches_of(arch);
        let settled = std::iter::once(Stance::Default)
            .chain(switches.iter().map(|&(mode, _)| Stance::Settled(mode)));
        for stance in settled {
            let (world, unit) = unit_in(&catalogs, arch, &switches, stance)?;
            let entity = &world.entities[&unit];
            let mode = match stance {
                Stance::Settled(mode) => Some(&arch.modes[mode]),
                _ => None,
            };
            let fires = [MoverType::Ground, MoverType::Air]
                .into_iter()
                .any(|mover| {
                    arch.weapon_against(mover, entity.active_siege_weapon())
                        .is_some()
                });
            assert_eq!(arch.stance_armed(mode), fires, "{} {stance:?}", arch.slug);
            assert_eq!(
                entity.settled_armed(arch),
                fires,
                "{} {stance:?}",
                arch.slug
            );
            if fires {
                armed += 1;
            } else {
                unarmed += 1;
            }
        }
    }
    assert!(armed > 0 && unarmed > 0, "{armed} armed, {unarmed} unarmed");
    Ok(())
}

/// The armed-only-in-a-mode case of P-6 through both channels: a unit whose default stance is
/// unarmed and whose mode replaces its ground weapon is armed in the settled stance from the
/// start of the switch into the mode and unarmed from the start of the switch out.
#[test]
fn a_unit_armed_only_in_its_mode_is_armed_for_the_stance_it_settles_in(
) -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (arch, mode) = catalogs
        .units
        .archetypes()
        .find_map(|arch| {
            let mode = arch
                .modes
                .iter()
                .position(|mode| matches!(mode.ground_weapon_override, Some(Some(_))))?;
            (!arch.stance_armed(None)).then(|| (arch.clone(), mode))
        })
        .ok_or("an archetype armed only in a mode")?;
    let switches = switches_of(&arch);
    for (stance, armed) in [
        (Stance::Default, false),
        (Stance::Entering(mode), true),
        (Stance::Settled(mode), true),
        (Stance::Leaving(mode), false),
    ] {
        let (world, unit) = unit_in(&catalogs, &arch, &switches, stance)?;
        assert_eq!(
            world.entities[&unit].settled_armed(&arch),
            armed,
            "{stance:?}"
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Plan `2026-11-24-attack-orders` (B-174, B-175): the simulation gives `AttackTarget` only to
// the named units that can attack the target (`Entity::can_attack`), and an unarmed
// attack-mover walks.
// ---------------------------------------------------------------------------------------------

/// The opponent of every test unit.
const THEM: PlayerId = PlayerId(1);

/// The targets of P-2, spawned in that order into a world from [`unit_in`]: an enemy ground
/// unit, an enemy air unit, an own ground unit (the same archetype as the enemy's) and the unit
/// itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    EnemyGround,
    EnemyAir,
    OwnGround,
    Itself,
}

/// A mobile ground archetype without modes and an air archetype, both found by property.
fn target_archetypes(
    catalogs: &RuntimeCatalogs,
) -> Result<(UnitArchetype, UnitArchetype), Box<dyn std::error::Error>> {
    let ground = catalogs
        .units
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
        })
        .ok_or("a mobile ground archetype without modes")?
        .clone();
    let air = catalogs
        .units
        .archetypes()
        .find(|arch| arch.mover_type == MoverType::Air)
        .ok_or("an air archetype")?
        .clone();
    Ok((ground, air))
}

/// Spawns the three other targets of [`Target`] into `world` around (20, 20), in a fixed order
/// so every copy of a test world names the same entities, and returns the target `which`.
fn spawn_targets(
    world: &mut SimWorld,
    unit: EntityId,
    (ground, air): (&UnitArchetype, &UnitArchetype),
    which: Target,
) -> Result<EntityId, Box<dyn std::error::Error>> {
    let enemy_ground = world.spawn_unit(ground.unit_type_id, THEM, at(32, 20))?;
    let enemy_air = world.spawn_unit(air.unit_type_id, THEM, at(20, 32))?;
    let own_ground = world.spawn_unit(ground.unit_type_id, ME, at(8, 20))?;
    Ok(match which {
        Target::EnemyGround => enemy_ground,
        Target::EnemyAir => enemy_air,
        Target::OwnGround => own_ground,
        Target::Itself => unit,
    })
}

/// Whether the unit of `world` can attack `target`, by `Entity::can_attack`.
fn can_attack(world: &SimWorld, unit: EntityId, target: EntityId) -> bool {
    let entity = &world.entities[&unit];
    let target = &world.entities[&target];
    let archetype = world
        .catalog
        .get(entity.unit_type_id)
        .expect("its archetype");
    let target_archetype = world
        .catalog
        .get(target.unit_type_id)
        .expect("its archetype");
    entity.can_attack(archetype, target, target_archetype)
}

/// P-2 (B-174, B-175): for every shipped archetype in each of its stances, an `AttackTarget`,
/// immediate or queued, on an enemy ground unit, an enemy air unit, an own unit or the unit
/// itself reaches the unit exactly when `Entity::can_attack` holds. A unit it reaches ends the
/// tick as if the order had been given to it directly; a unit it does not reach ends the tick
/// equal, field for field, to the same unit in a world that received no command. The owner is
/// not read, and a unit never attacks itself.
#[test]
fn the_simulation_gives_an_attack_only_to_a_unit_that_can_attack_its_target(
) -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (ground, air) = target_archetypes(&catalogs)?;
    let (mut taken, mut refused, mut own_taken) = (0, 0, 0);
    for arch in catalogs
        .units
        .archetypes()
        .filter(|arch| !needs_a_geyser(arch))
    {
        let switches = switches_of(arch);
        for stance in stances(&switches) {
            let mut verdicts = Vec::new();
            for which in [
                Target::EnemyGround,
                Target::EnemyAir,
                Target::OwnGround,
                Target::Itself,
            ] {
                for queue in [false, true] {
                    let label = format!("{} {stance:?} {which:?} queue {queue}", arch.slug);
                    let build =
                        || -> Result<(SimWorld, EntityId, EntityId), Box<dyn std::error::Error>> {
                            let (mut world, unit) = unit_in(&catalogs, arch, &switches, stance)?;
                            let target = spawn_targets(&mut world, unit, (&ground, &air), which)?;
                            Ok((world, unit, target))
                        };
                    let (mut commanded, unit, target) = build()?;
                    let (mut untouched, ..) = build()?;
                    let (mut forced, ..) = build()?;
                    let takes = can_attack(&commanded, unit, target);
                    let order = UnitOrder::attack(target);
                    forced
                        .entities
                        .get_mut(&unit)
                        .ok_or("the forced unit")?
                        .issue_order(order.clone(), queue)?;
                    step_with(&mut commanded, vec![command(vec![unit], order, queue)]);
                    step(&mut untouched);
                    step(&mut forced);
                    let expected = if takes { &forced } else { &untouched };
                    assert_eq!(
                        commanded.entities[&unit], expected.entities[&unit],
                        "{label}: takes {takes}"
                    );
                    assert_eq!(
                        commanded.compute_checksum(),
                        expected.compute_checksum(),
                        "{label}: takes {takes}"
                    );
                    verdicts.push((which, takes));
                    if takes {
                        taken += 1;
                        own_taken += usize::from(which == Target::OwnGround);
                    } else {
                        refused += 1;
                    }
                }
            }
            let of = |which: Target| {
                verdicts
                    .iter()
                    .find(|(target, _)| *target == which)
                    .map(|(_, takes)| *takes)
            };
            let label = format!("{} {stance:?}", arch.slug);
            assert_eq!(of(Target::Itself), Some(false), "{label}: never itself");
            assert_eq!(
                of(Target::OwnGround),
                of(Target::EnemyGround),
                "{label}: the owner is not read"
            );
        }
    }
    assert!(taken > 0 && refused > 0, "{taken} taken, {refused} refused");
    assert!(own_taken > 0, "an own unit is attacked");
    Ok(())
}

/// A mobile ground archetype without modes that is armed against ground and not against air,
/// and a mobile unarmed archetype whose A command is Scan Move; both found by property.
fn ground_gunner_and_unarmed(
    catalogs: &RuntimeCatalogs,
) -> Result<(UnitArchetype, UnitArchetype), Box<dyn std::error::Error>> {
    let gunner = catalogs
        .units
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && arch.mover_type == MoverType::Ground
                && arch.modes.is_empty()
                && arch.weapon_against(MoverType::Ground, None).is_some()
                && arch.weapon_against(MoverType::Air, None).is_none()
        })
        .ok_or("a ground-only armed walker")?
        .clone();
    let unarmed = catalogs
        .units
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && arch.stance_orders(None).attack == sim_types::AttackCommand::ScanMove
        })
        .ok_or("an unarmed scan-mover")?
        .clone();
    Ok((gunner, unarmed))
}

/// P-3 (`ToSelection`, 5.0.14 `core` AbilData:127-129): one `AttackTarget` command naming a
/// ground-only armed unit and an unarmed one reaches neither when aimed at an enemy air unit,
/// and only the armed one when aimed at an enemy ground unit.
#[test]
fn one_attack_command_reaches_each_named_unit_that_can_attack_the_target(
) -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (gunner, unarmed) = ground_gunner_and_unarmed(&catalogs)?;
    let (ground, air) = target_archetypes(&catalogs)?;
    for (which, gunner_takes) in [(Target::EnemyAir, false), (Target::EnemyGround, true)] {
        let build =
            || -> Result<(SimWorld, [EntityId; 2], EntityId), Box<dyn std::error::Error>> {
                let (mut world, armed) = unit_in(&catalogs, &gunner, &[], Stance::Default)?;
                let walker = world.spawn_unit(unarmed.unit_type_id, ME, at(24, 24))?;
                let target = spawn_targets(&mut world, armed, (&ground, &air), which)?;
                Ok((world, [armed, walker], target))
            };
        let (mut world, [armed, walker], target) = build()?;
        let (mut untouched, ..) = build()?;
        step_with(
            &mut world,
            vec![command(
                vec![armed, walker],
                UnitOrder::attack(target),
                false,
            )],
        );
        step(&mut untouched);
        assert_eq!(
            world.entities[&walker], untouched.entities[&walker],
            "{which:?}: the unarmed unit is left as it was"
        );
        assert_eq!(
            world.entities[&armed].order_queue.front() == Some(&UnitOrder::attack(target)),
            gunner_takes,
            "{which:?}: the armed unit"
        );
        if !gunner_takes {
            assert_eq!(
                world.entities[&armed], untouched.entities[&armed],
                "{which:?}: the armed unit is left as it was"
            );
        }
    }
    Ok(())
}

/// P-5 (plan §2.2 rule 2, §3.3): an unarmed mobile unit given `AttackMove` past enemies walks to
/// the point and acquires nothing, while an armed unit given the same order beside it acquires.
/// This pins the simulation's rule B-174's and B-186's fixes rely on.
#[test]
fn an_unarmed_attack_mover_walks_past_enemies_and_acquires_nothing(
) -> Result<(), Box<dyn std::error::Error>> {
    let catalogs = sim_fixtures::shipped_catalogs();
    let (gunner, unarmed) = ground_gunner_and_unarmed(&catalogs)?;
    let mut world = flat_world(0x5a_0200);
    let walker = world.spawn_unit(unarmed.unit_type_id, ME, at(6, 20))?;
    let armed = world.spawn_unit(gunner.unit_type_id, ME, at(6, 26))?;
    // Unarmed ground enemies beside the path: candidates for either unit's scan (the armed
    // unit's weapon reaches ground), and no threat.
    let prey = catalogs
        .units
        .archetypes()
        .find(|arch| {
            !arch.is_immobile_by_default()
                && arch.mover_type == MoverType::Ground
                && !arch.stance_armed(None)
        })
        .ok_or("an unarmed ground walker")?;
    for x in [18, 22, 26] {
        world.spawn_unit(prey.unit_type_id, THEM, at(x, 23))?;
    }
    let goal = at(40, 20);
    step_with(
        &mut world,
        vec![
            command(vec![walker], UnitOrder::AttackMove { target: goal }, false),
            command(
                vec![armed],
                UnitOrder::AttackMove { target: at(40, 26) },
                false,
            ),
        ],
    );
    let mut armed_acquired = false;
    for _ in 0..PATIENCE * 2 {
        assert_eq!(
            world.entities[&walker].current_target, None,
            "the walker acquires nothing"
        );
        armed_acquired |= world.entities[&armed].current_target.is_some();
        if world.entities[&walker].order_queue.is_empty() {
            break;
        }
        step(&mut world);
    }
    assert!(
        world.entities[&walker].order_queue.is_empty(),
        "the walker finished its attack-move"
    );
    assert!(
        world.entities[&walker].position.distance(goal) < SimScalar::from_num(2),
        "the walker arrived: {:?}",
        world.entities[&walker].position
    );
    assert!(armed_acquired, "the armed unit beside it acquired");
    Ok(())
}
