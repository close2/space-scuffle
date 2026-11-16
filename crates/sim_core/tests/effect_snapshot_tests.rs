//! # Integration Test: the effect presentation facts of the render snapshot
//!
//! Plan `2026-11-09-effect-visualization` WP2. The snapshot writer publishes, from state the
//! tick already keeps, what the client's effect system needs: each bubble's concealment,
//! stance (with the progress of a mode switch), status flags and barrier fraction; one
//! `SplashVisual` per splash search with the weapon's zones; the source unit type and weapon
//! of every weapon event; and the open area spells (point persistents that search) as areas.
//!
//! Plan `2026-11-17-effect-snapshot-identity` WP2 adds the identities: the attacker and the
//! struck entity of every instant hit and blow (`StrikeEnds`), the catalog statuses of every
//! published bubble (`RenderSnapshot::statuses`), and a `SpellImpact` where a spell missile's
//! impact ran (none for a missile that lost its target).
//!
//! These tests drive the shipped catalog (and scratch archetypes cloned from it) through
//! `SimWorld::step` and `SimWorld::execute_ability`. Slugs are read from data; the simulation
//! source names none of them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]

#[path = "support/scratch_storm.rs"]
mod scratch_storm;

use scratch_storm::ScratchStorm;
use sim_core::entity::{ActiveBuff, ATTACK_DASH_BUFF_ID, OVERCLOCK_BUFF_ID};
use sim_core::{SimWorld, SplashSpec, UnitArchetype, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySource, AbilitySpec, AbilityTargetKind, BubbleRenderInstance,
    CombatEventVisual, Concealment, EffectContext, EffectSource, EffectTarget, EntityId, MoverType,
    PlayerId, RenderSnapshot, ShapeKind, SimScalar, SimVec2, StatusView, StrikeEnds, Tick,
    TickBundle, UnitOrder, UnitTypeId, WeaponKey, WeaponSlot,
};

const ATTACKER: PlayerId = PlayerId(0);
const DEFENDER: PlayerId = PlayerId(1);
/// Ticks a test waits for something that should happen within a few attack periods.
const PATIENCE: u32 = 300;

/// A 64 × 64 flat world on the shipped catalog.
fn flat_world(seed: u64) -> SimWorld {
    let map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0)))
        .expect("invariant: a flat 64x64 map is valid");
    sim_fixtures::world(map, seed)
}

fn at(x: f64, y: f64) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}

fn step(world: &mut SimWorld) -> RenderSnapshot {
    world.step(&TickBundle {
        tick: Tick(world.current_tick),
        commands: Vec::new(),
        state_checksum: None,
    })
}

fn bubble(snapshot: &RenderSnapshot, id: EntityId) -> &BubbleRenderInstance {
    snapshot
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == id)
        .expect("the entity has a bubble")
}

fn mode_index(arch: &UnitArchetype, name: &str) -> Option<u8> {
    arch.modes
        .iter()
        .position(|mode| mode.mode_name == name)
        .map(|index| u8::try_from(index).expect("few modes"))
}

/// Index of the ability of `arch` that switches into `mode`.
fn switch_ability(arch: &UnitArchetype, mode: &str) -> u8 {
    let index = arch
        .abilities
        .iter()
        .position(|ability| {
            matches!(&ability.effect, AbilityEffectSpec::SwitchMode { mode_name } if mode_name == mode)
        })
        .expect("the mode has a switch ability");
    u8::try_from(index).expect("few abilities")
}

/// The shipped archetype whose mode `predicate` holds, with that mode's name.
fn moded(
    world: &SimWorld,
    predicate: impl Fn(&sim_types::UnitModeSpec) -> bool,
) -> (UnitArchetype, String) {
    world
        .catalog
        .archetypes()
        .find_map(|arch| {
            arch.modes
                .iter()
                .find(|mode| predicate(mode))
                .map(|mode| (arch.clone(), mode.mode_name.clone()))
        })
        .expect("the shipped catalog has such a mode")
}

/// Progress of a published mode switch, as the client computes it (plan D-1).
fn progress(remaining: u16, nominal: u16) -> f32 {
    ((f32::from(nominal) - f32::from(remaining)) / f32::from(nominal)).clamp(0.0, 1.0)
}

#[test]
fn burrowing_publishes_its_progress_and_then_burrowed() {
    let mut world = flat_world(0x0e_ff01);
    let (arch, mode) = moded(&world, |mode| mode.burrowed);
    let spec = arch.get_mode(&mode).expect("the mode exists").clone();
    let index = mode_index(&arch, &mode);
    let unit = world
        .spawn_unit(arch.unit_type_id, ATTACKER, at(20.0, 20.0))
        .expect("spawn the burrower");
    assert_eq!(
        bubble(&step(&mut world), unit).concealment,
        Concealment::None
    );

    assert!(world.execute_ability(unit, switch_ability(&arch, &mode), None, None));
    let mut seen = Vec::new();
    let burrowed = loop {
        let snapshot = step(&mut world);
        let record = bubble(&snapshot, unit).clone();
        match record.stance.transition {
            Some(transition) => {
                assert_eq!(transition.entering, index, "it enters the burrowed mode");
                assert_eq!(transition.nominal_ticks, spec.cast_ticks);
                assert_eq!(record.stance.mode, None, "still in the default stance");
                assert_eq!(record.concealment, Concealment::None, "not hidden before");
                seen.push(progress(
                    transition.remaining_ticks,
                    transition.nominal_ticks,
                ));
            }
            None => break record,
        }
        assert!(seen.len() < PATIENCE as usize, "the channel ends");
    };
    assert!(seen.len() >= usize::from(spec.cast_ticks) - 1, "{seen:?}");
    assert!(seen.windows(2).all(|pair| pair[0] <= pair[1]), "{seen:?}");
    assert!(seen[0] < 0.1, "progress starts near 0: {seen:?}");
    assert!(
        *seen.last().expect("some progress") > 0.95,
        "and reaches 1: {seen:?}"
    );
    assert_eq!(burrowed.stance.mode, index);
    assert_eq!(burrowed.concealment, Concealment::Burrowed);

    assert!(world.execute_ability(unit, switch_ability(&arch, &mode), None, None));
    let leaving = step(&mut world);
    let transition = bubble(&leaving, unit)
        .stance
        .transition
        .expect("the unburrow channel is published");
    assert_eq!(
        transition.entering, None,
        "it returns to the default stance"
    );
    assert_eq!(transition.nominal_ticks, spec.leave_cast_ticks);
    let surfaced = (0..PATIENCE)
        .map(|_| bubble(&step(&mut world), unit).clone())
        .find(|record| record.stance.transition.is_none())
        .expect("the unburrow ends");
    assert_eq!(surfaced.stance.mode, None);
    assert_eq!(surfaced.concealment, Concealment::None);
}

#[test]
fn an_anchoring_mode_is_a_stance_not_a_concealment() {
    let mut world = flat_world(0x0e_ff02);
    let (arch, mode) = moded(&world, |mode| mode.is_immobile && !mode.is_cloaked);
    let spec = arch.get_mode(&mode).expect("the mode exists").clone();
    let unit = world
        .spawn_unit(arch.unit_type_id, ATTACKER, at(20.0, 20.0))
        .expect("spawn the unit");
    assert!(world.execute_ability(unit, switch_ability(&arch, &mode), None, None));
    let first = step(&mut world);
    let transition = bubble(&first, unit).stance.transition.expect("a channel");
    assert_eq!(transition.entering, mode_index(&arch, &mode));
    assert_eq!(transition.nominal_ticks, spec.cast_ticks);
    let settled = (0..PATIENCE)
        .map(|_| bubble(&step(&mut world), unit).clone())
        .find(|record| record.stance.transition.is_none())
        .expect("the channel ends");
    assert_eq!(settled.stance.mode, mode_index(&arch, &mode));
    assert_eq!(settled.concealment, Concealment::None);
}

/// The slot a shipped archetype's ground weapon is authored under (plan §5.3).
fn ground_slot(arch: &UnitArchetype) -> WeaponSlot {
    if matches!(arch.shape, ShapeKind::BuildingDome { .. }) || arch.ground_weapon != arch.weapon {
        WeaponSlot::Ground
    } else {
        WeaponSlot::Primary
    }
}

/// One weapon of the shipped catalog: who carries it, what it is, how it is keyed, and the
/// mode a unit must enter to fire it.
struct ShippedWeapon {
    arch: UnitArchetype,
    weapon: WeaponSpec,
    key: WeaponKey,
    plane: MoverType,
    mode: Option<String>,
}

/// Every weapon a shipped mobile unit can fire: its air weapon, the weapon its ground plane
/// resolves to, and each mode's ground weapon override.
fn shipped_weapons(world: &SimWorld) -> Vec<ShippedWeapon> {
    let mut weapons = Vec::new();
    for arch in world.catalog.archetypes() {
        if matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
            continue;
        }
        let ground = arch.ground_weapon.clone().or_else(|| arch.weapon.clone());
        if let Some(weapon) = ground {
            weapons.push(ShippedWeapon {
                arch: arch.clone(),
                weapon,
                key: WeaponKey {
                    slot: ground_slot(arch),
                    mode: None,
                },
                plane: MoverType::Ground,
                mode: None,
            });
        }
        if let Some(weapon) = arch.air_weapon.clone() {
            weapons.push(ShippedWeapon {
                arch: arch.clone(),
                weapon,
                key: WeaponKey {
                    slot: WeaponSlot::Air,
                    mode: None,
                },
                plane: MoverType::Air,
                mode: None,
            });
        }
        for mode in &arch.modes {
            if let Some(Some(weapon)) = mode.ground_weapon_override.clone() {
                weapons.push(ShippedWeapon {
                    arch: arch.clone(),
                    weapon,
                    key: WeaponKey {
                        slot: ground_slot(arch),
                        mode: mode_index(arch, &mode.mode_name),
                    },
                    plane: MoverType::Ground,
                    mode: Some(mode.mode_name.clone()),
                });
            }
        }
    }
    weapons
}

/// A shipped mobile unit on `plane` that `weapon` may strike.
fn target_for(world: &SimWorld, weapon: &WeaponSpec, plane: MoverType) -> Option<UnitTypeId> {
    world
        .catalog
        .archetypes()
        .find(|arch| {
            !matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && arch.mover_type == plane
                && weapon.target_flags.intersects(arch.attributes)
                && arch.modes.iter().all(|mode| !mode.is_cloaked)
        })
        .map(|arch| arch.unit_type_id)
}

/// What a [`duel`] saw: `ATTACKER`'s weapon events, and the attacker and the target it fought
/// (`None` when the catalog has no target for the weapon).
struct Duel {
    events: Vec<CombatEventVisual>,
    ends: Option<StrikeEnds>,
}

/// The renderer events of `ATTACKER` while its one unit fires `shipped` at a sturdy target in a
/// fresh world of `seed`, after entering the weapon's mode if it has one. See [`duel`].
fn fire(shipped: &ShippedWeapon, seed: u64) -> Duel {
    let mut world = flat_world(seed);
    let attacker = world
        .spawn_unit(shipped.arch.unit_type_id, ATTACKER, at(20.0, 20.0))
        .expect("spawn the attacker");
    if let Some(mode) = &shipped.mode {
        assert!(world.execute_ability(attacker, switch_ability(&shipped.arch, mode), None, None));
        for _ in 0..PATIENCE {
            if world.entities[&attacker].channeling_remaining_ticks == 0 {
                break;
            }
            step(&mut world);
        }
    }
    duel(&mut world, shipped, attacker)
}

/// Spawns a sturdy, holding target that `shipped` may strike within its range of `attacker`
/// (which carries it, at (20, 20)), orders the attack and returns `ATTACKER`'s weapon events
/// from the first one until the shot's splash has landed (or, without splash, that first tick),
/// or 60 ticks later. No events when the catalog has no such target or the attacker never
/// fires.
fn duel(world: &mut SimWorld, shipped: &ShippedWeapon, attacker: EntityId) -> Duel {
    let Some(target_type) = target_for(world, &shipped.weapon, shipped.plane) else {
        return Duel {
            events: Vec::new(),
            ends: None,
        };
    };
    let target_arch = world.catalog.get(target_type).expect("a target").clone();
    let min = f64::from(shipped.weapon.min_range.to_f32());
    let max = f64::from(shipped.weapon.max_range.to_f32());
    let gap = if min > 0.0 {
        (min + 0.5).min(max)
    } else {
        (max * 0.5).min(1.0)
    };
    let distance = f64::from(shipped.arch.collision_radius.to_f32())
        + f64::from(target_arch.collision_radius.to_f32())
        + gap;
    let target = world
        .spawn_unit(target_type, DEFENDER, at(20.0 + distance, 20.0))
        .expect("spawn the target");
    {
        let victim = world.entities.get_mut(&target).expect("the target");
        victim.max_hp = SimScalar::from_num(100_000);
        victim.hp = victim.max_hp;
        victim
            .issue_order(UnitOrder::HoldPosition, false)
            .expect("hold");
    }
    world
        .entities
        .get_mut(&attacker)
        .expect("the attacker")
        .issue_order(UnitOrder::attack(target), false)
        .expect("attack");
    let mut events = Vec::new();
    let mut since_first = 0;
    for _ in 0..PATIENCE {
        let snapshot = step(world);
        if !events.is_empty() {
            since_first += 1;
        }
        events.extend(
            snapshot
                .combat_events
                .into_iter()
                .filter(|event| event.owner() == ATTACKER && event.source().is_some()),
        );
        let searches = events
            .iter()
            .filter(|event| matches!(event, CombatEventVisual::SplashVisual { .. }))
            .count();
        // A line whose searches land on several ticks (B-85) has splashed once its last has.
        let done = match shipped.weapon.splash.as_ref().map(|splash| &splash.shape) {
            Some(sim_types::SplashShape::Line(line)) => searches >= line.offsets.len(),
            Some(sim_types::SplashShape::Circle) => searches > 0,
            None => !events.is_empty(),
        };
        if done || since_first > 60 {
            break;
        }
    }
    Duel {
        events,
        ends: Some(StrikeEnds { attacker, target }),
    }
}

#[test]
fn every_weapon_event_carries_its_source() {
    let world = flat_world(0x0e_ff03);
    let mut fired = 0;
    let mut silent = Vec::new();
    for (seed, shipped) in (1_u64..).zip(shipped_weapons(&world)) {
        let events = fire(&shipped, seed).events;
        if events.is_empty() {
            silent.push(format!("{} {:?}", shipped.arch.slug, shipped.key));
            continue;
        }
        fired += 1;
        let expected = EffectSource {
            unit_type: shipped.arch.unit_type_id,
            weapon: shipped.key,
        };
        for event in &events {
            assert_eq!(
                event.source(),
                Some(expected),
                "{} {:?}: {event:?}",
                shipped.arch.slug,
                shipped.key
            );
        }
    }
    assert!(
        fired * 4 >= (fired + silent.len()) * 3,
        "most shipped weapons fire in the duel; silent: {silent:?}"
    );
}

/// The zones a `SplashVisual` of `splash` publishes: innermost, middle, outermost.
fn zones_of(splash: &SplashSpec) -> [f32; 3] {
    let radius = |index: usize| splash.zones[index].radius.to_f32();
    [
        radius(0),
        radius((splash.zones.len() - 1) / 2),
        splash.reach().to_f32(),
    ]
}

#[test]
fn each_shipped_splash_weapon_publishes_its_zones() {
    let world = flat_world(0x0e_ff04);
    let splashers: Vec<_> = shipped_weapons(&world)
        .into_iter()
        .filter(|shipped| shipped.weapon.splash.is_some())
        .collect();
    assert!(
        !splashers.is_empty(),
        "the shipped catalog has splash weapons"
    );
    for (seed, shipped) in (100_u64..).zip(&splashers) {
        let splash = shipped.weapon.splash.as_ref().expect("filtered on splash");
        let events = fire(shipped, seed).events;
        let searches: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                CombatEventVisual::SplashVisual { zones, line, .. } => Some((*zones, *line)),
                _ => None,
            })
            .collect();
        assert!(
            !searches.is_empty(),
            "{} {:?} splashes: {events:?}",
            shipped.arch.slug,
            shipped.key
        );
        let expected_points = match &splash.shape {
            sim_types::SplashShape::Circle => 1,
            sim_types::SplashShape::Line(line) => line.offsets.len(),
        };
        assert_eq!(searches.len(), expected_points, "{}", shipped.arch.slug);
        for (zones, line) in searches {
            assert_eq!(zones, zones_of(splash), "{}", shipped.arch.slug);
            assert_eq!(
                line.is_some(),
                matches!(splash.shape, sim_types::SplashShape::Line(_)),
                "only a line search carries a line"
            );
        }
    }
}

#[test]
fn a_line_splash_emits_one_event_per_offset() {
    line_splash_emits_one_event_per_offset(0);
}

/// B-85: a line whose searches land `period_ticks` apart publishes each search when it lands,
/// numbered by its place in the whole line, with the whole line and the weapon's source.
#[test]
fn a_spaced_line_splash_emits_each_search_as_it_lands() {
    line_splash_emits_one_event_per_offset(2);
}

/// A scratch unit fires a three-search line (offsets 1, 2.5 and 4, searches `period_ticks`
/// apart); its renderer events carry one `SplashVisual` per search, in order.
fn line_splash_emits_one_event_per_offset(period_ticks: u16) {
    let mut world = flat_world(0x0e_ff05);
    let line: SplashSpec = ron::from_str(&format!(
        "(zones: [(radius: 0.5, fraction: 1.0), (radius: 0.75, fraction: 0.5)],
          shape: Line((origin: Attacker, axis: Along, offsets: [1.0, 2.5, 4.0],
                       period_ticks: {period_ticks})),
          friendly_fire: false)"
    ))
    .expect("a valid line splash in RON");
    let base = world
        .catalog
        .archetypes()
        .find(|arch| {
            arch.weapon.as_ref().is_some_and(|weapon| {
                weapon.projectile_speed.is_none() && weapon.min_range == SimScalar::ZERO
            }) && arch.ground_weapon == arch.weapon
                && arch.max_speed > SimScalar::ZERO
                && arch.modes.is_empty()
        })
        .expect("a shipped unit with an instant ground weapon")
        .clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("a free unit type id");
    let mut scratch = base;
    scratch.unit_type_id = free;
    "effect_test_line_splasher".clone_into(&mut scratch.slug);
    scratch.name.clone_from(&scratch.slug);
    for weapon in [&mut scratch.weapon, &mut scratch.ground_weapon]
        .into_iter()
        .flatten()
    {
        weapon.splash = Some(line.clone());
        weapon.max_range = SimScalar::from_num(3);
    }
    world.catalog.register(scratch.clone());
    let shipped = ShippedWeapon {
        weapon: scratch.weapon.clone().expect("armed"),
        arch: scratch,
        key: WeaponKey {
            slot: WeaponSlot::Primary,
            mode: None,
        },
        plane: MoverType::Ground,
        mode: None,
    };
    let attacker = world
        .spawn_unit(shipped.arch.unit_type_id, ATTACKER, at(20.0, 20.0))
        .expect("spawn the attacker");
    let events = duel(&mut world, &shipped, attacker).events;
    let searches: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            CombatEventVisual::SplashVisual {
                center,
                zones,
                line,
                source,
                ..
            } => Some((
                *center,
                *zones,
                line.expect("a line search carries its line"),
                *source,
            )),
            _ => None,
        })
        .collect();
    assert_eq!(searches.len(), 3, "one event per offset: {events:?}");
    for (index, (center, zones, view, source)) in (0_u8..).zip(&searches) {
        assert_eq!(*zones, [0.5, 0.5, 0.75]);
        assert_eq!(view.index, index);
        assert_eq!(view.count, 3);
        assert!((view.spacing - 1.5).abs() < 1e-4, "{view:?}");
        let length =
            ((view.to[0] - view.from[0]).powi(2) + (view.to[1] - view.from[1]).powi(2)).sqrt();
        assert!(
            (length - 4.0).abs() < 1e-3,
            "the line reaches the largest offset: {view:?}"
        );
        let offset = [1.0_f32, 2.5, 4.0][usize::from(index)];
        let along =
            ((center[0] - view.from[0]).powi(2) + (center[1] - view.from[1]).powi(2)).sqrt();
        assert!(
            (along - offset).abs() < 1e-3,
            "point {index} lies at its offset"
        );
        assert_eq!(source.unit_type, shipped.arch.unit_type_id);
    }
}

/// Registers a clone of a shipped mobile unit whose first ability casts the scratch storm
/// (`support/scratch_storm.rs`), which it installs.
fn register_storm_caster(world: &mut SimWorld) -> UnitTypeId {
    ScratchStorm::GAME.install(world);
    let mut arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.max_speed > SimScalar::ZERO && arch.modes.is_empty())
        .expect("a shipped mobile unit")
        .clone();
    let free = (1..=u16::MAX)
        .map(UnitTypeId)
        .find(|id| world.catalog.get(*id).is_err())
        .expect("a free unit type id");
    arch.unit_type_id = free;
    "effect_test_storm_caster".clone_into(&mut arch.slug);
    arch.name.clone_from(&arch.slug);
    arch.abilities = vec![AbilitySpec {
        name: "Effect Test Storm".to_owned(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: SimScalar::from_num(20),
        effect: AbilityEffectSpec::Effect("scratch_storm".to_owned()),
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }];
    world.catalog.register(arch);
    free
}

/// A storm is an area from its cast until its last search (plan
/// `2026-11-10-spellcaster-mechanics` D-19): six periodic searches 10.2 ticks apart close it
/// 61 ticks after the cast, the hazard zone's replacement (D-12).
#[test]
fn a_storm_is_an_area_until_it_closes() {
    let mut world = flat_world(0x0e_ff07);
    let caster_type = register_storm_caster(&mut world);
    let caster = world
        .spawn_unit(caster_type, ATTACKER, at(20.0, 20.0))
        .expect("spawn the caster");
    assert_eq!(step(&mut world).areas, []);
    assert!(world.execute_ability(caster, 0, Some(at(24.0, 20.0)), None));
    let mut remaining = Vec::new();
    for _ in 0..PATIENCE {
        let snapshot = step(&mut world);
        let Some(area) = snapshot.areas.first() else {
            break;
        };
        assert_eq!(snapshot.areas.len(), 1);
        assert_eq!(area.owner, ATTACKER);
        assert_eq!(area.center[..2], [24.0, 20.0]);
        assert!((area.radius - 1.5).abs() < 1e-4);
        assert_eq!(area.total_ticks, 61);
        assert_eq!(
            area.source,
            Some(AbilitySource {
                unit_type: caster_type,
                ability: 0,
            })
        );
        remaining.push(area.remaining_ticks);
    }
    // One frame a tick after the cast's: 60 left after the first tick, down to 1 on the tick
    // before the last search, which closes it.
    assert_eq!(
        remaining,
        (1..=60).rev().collect::<Vec<u16>>(),
        "it lasts until its last search"
    );
    assert_eq!(step(&mut world).areas, [], "and stays gone");
}

#[test]
fn status_flags_follow_buffs() {
    let mut world = flat_world(0x0e_ff08);
    let arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.max_speed > SimScalar::ZERO && arch.overclock.is_some())
        .expect("a shipped overclock unit")
        .clone();
    let unit = world
        .spawn_unit(arch.unit_type_id, ATTACKER, at(20.0, 20.0))
        .expect("spawn the unit");
    let plain = bubble(&step(&mut world), unit).status;
    assert_eq!(plain, sim_types::StatusFlags::default());

    let buff = |buff_id, speed: i32| ActiveBuff {
        buff_id,
        remaining_ticks: 100,
        speed_multiplier: SimScalar::from_num(speed),
        attack_rate_multiplier: SimScalar::ONE,
    };
    world
        .entities
        .get_mut(&unit)
        .expect("the unit")
        .active_buffs
        .push(buff(OVERCLOCK_BUFF_ID, 2));
    let status = bubble(&step(&mut world), unit).status;
    assert!(status.overclock && !status.dash && !status.slowed);

    {
        let entity = world.entities.get_mut(&unit).expect("the unit");
        entity.active_buffs.clear();
        entity.active_buffs.push(buff(ATTACK_DASH_BUFF_ID, 2));
    }
    let status = bubble(&step(&mut world), unit).status;
    assert!(status.dash && !status.overclock && !status.slowed);

    // A slowing status of the effect engine (Concussive Shells' hit, plan
    // `2026-11-05-sc2-deviation-fixes` WP13d) raises the slow flag.
    world
        .entities
        .get_mut(&unit)
        .expect("the unit")
        .active_buffs
        .clear();
    let slow = world
        .catalog
        .spells()
        .effect_id("concussive_shells_slow")
        .expect("the shipped concussive slow");
    let position = world.entities[&unit].position;
    world.run_effect(
        slow,
        EffectContext {
            owner: ATTACKER,
            caster: None,
            ability: AbilitySource::default(),
            origin: position,
            source_point: position,
            target: EffectTarget::Unit(unit),
        },
    );
    let status = bubble(&step(&mut world), unit).status;
    assert!(status.slowed && !status.dash && !status.overclock);
}

#[test]
fn the_barrier_publishes_its_hits_and_what_it_has_left() {
    let mut world = flat_world(0x0e_ff09);
    let arch = world
        .catalog
        .archetypes()
        .find(|arch| arch.capabilities.barrier.is_some())
        .expect("a shipped barrier unit")
        .clone();
    let shooter_arch = world
        .catalog
        .archetypes()
        .find(|candidate| {
            candidate.max_speed > SimScalar::ZERO
                && candidate.modes.is_empty()
                && candidate.weapon.as_ref().is_some_and(|weapon| {
                    weapon.splash.is_none()
                        && weapon.projectile_speed.is_none()
                        && weapon.target_flags.intersects(arch.attributes)
                })
                && candidate.ground_weapon == candidate.weapon
        })
        .expect("a shipped instant-hit unit")
        .clone();
    let holder = world
        .spawn_unit(arch.unit_type_id, DEFENDER, at(20.0, 20.0))
        .expect("spawn the barrier unit");
    world
        .entities
        .get_mut(&holder)
        .expect("the holder")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold");
    let range = f64::from(
        shooter_arch
            .weapon
            .as_ref()
            .expect("armed")
            .max_range
            .to_f32(),
    );
    let distance = f64::from(arch.collision_radius.to_f32())
        + f64::from(shooter_arch.collision_radius.to_f32())
        + (range * 0.5).min(1.0);
    let shooter = world
        .spawn_unit(
            shooter_arch.unit_type_id,
            ATTACKER,
            at(20.0 - distance, 20.0),
        )
        .expect("spawn the shooter");
    world
        .entities
        .get_mut(&shooter)
        .expect("the shooter")
        .issue_order(UnitOrder::attack(holder), false)
        .expect("attack");
    let mut hits = 0;
    let mut up_without_hit = 0;
    for _ in 0..PATIENCE {
        let snapshot = step(&mut world);
        let Some(record) = snapshot.bubbles.iter().find(|b| b.entity_id == holder) else {
            break;
        };
        let state = world.entities[&holder].barrier;
        assert_eq!(record.status.barrier_up, state.is_raised());
        if record.status.barrier_up {
            assert!(record.barrier_ratio > 0.0 && record.barrier_ratio <= 1.0);
        } else {
            assert_eq!(record.barrier_ratio, 0.0);
        }
        if record.status.barrier_hit {
            hits += 1;
        } else if record.status.barrier_up {
            up_without_hit += 1;
        }
    }
    assert!(hits > 0, "the barrier absorbed a hit");
    assert!(
        up_without_hit > 0,
        "a raised barrier between hits is not hit"
    );
}

/// Two worlds fed the same inputs publish equal records, and writing a second snapshot from
/// one world after a tick publishes the same bubbles and areas without its drained events.
#[test]
fn equal_worlds_publish_equal_records() {
    let build = || {
        let mut world = flat_world(0x0e_ff0a);
        let caster_type = register_storm_caster(&mut world);
        let (arch, mode) = moded(&world, |mode| mode.burrowed);
        let burrower = world
            .spawn_unit(arch.unit_type_id, ATTACKER, at(20.0, 20.0))
            .expect("spawn");
        let caster = world
            .spawn_unit(caster_type, DEFENDER, at(26.0, 20.0))
            .expect("spawn");
        assert!(world.execute_ability(burrower, switch_ability(&arch, &mode), None, None));
        assert!(world.execute_ability(caster, 0, Some(at(21.0, 20.0)), None));
        world
    };
    let (mut first, mut second) = (build(), build());
    for _ in 0..40 {
        assert_eq!(step(&mut first), step(&mut second));
    }
    let checksum = first.compute_checksum();
    let mut again = RenderSnapshot::default();
    first.write_render_snapshot(&mut again);
    assert_eq!(
        first.compute_checksum(),
        checksum,
        "writing changes no state"
    );
    let mut stepped = step(&mut second);
    assert_eq!(stepped.tick, Tick(second.current_tick));
    let mut rewritten = RenderSnapshot::default();
    second.write_render_snapshot(&mut rewritten);
    assert_eq!(rewritten.combat_events, [], "events drain once");
    for bubble in &mut stepped.bubbles {
        bubble.status.barrier_hit = false;
    }
    stepped.combat_events.clear();
    assert_eq!(rewritten, stepped);
    assert_eq!(rewritten.areas.len(), 1, "the storm is still open");
}

/// Whether `event` is an instant hit or a blow, the events that name their strike ends.
const fn is_strike(event: &CombatEventVisual) -> bool {
    matches!(
        event,
        CombatEventVisual::HitscanTracer { .. }
            | CombatEventVisual::BeamVisual { .. }
            | CombatEventVisual::MeleeSlashVisual { .. }
    )
}

/// The strike ends `event` names; `None` for an event without them.
const fn strike_ends(event: &CombatEventVisual) -> Option<StrikeEnds> {
    match event {
        CombatEventVisual::HitscanTracer { ends, .. }
        | CombatEventVisual::BeamVisual { ends, .. }
        | CombatEventVisual::MeleeSlashVisual { ends, .. } => *ends,
        _ => None,
    }
}

/// B-125, B-126 (plan `2026-11-17-effect-snapshot-identity` D-2): every instant hit and blow a
/// shipped weapon fires names the attacker that fired it and the target it struck.
#[test]
fn every_instant_hit_and_blow_names_its_attacker_and_target() {
    let world = flat_world(0x0e_ff0b);
    let mut strikes = 0;
    for (seed, shipped) in (200_u64..).zip(shipped_weapons(&world)) {
        let duel = fire(&shipped, seed);
        for event in duel.events.iter().filter(|event| is_strike(event)) {
            strikes += 1;
            assert_eq!(
                strike_ends(event),
                duel.ends,
                "{} {:?}: {event:?}",
                shipped.arch.slug,
                shipped.key
            );
        }
    }
    assert!(strikes > 0, "the shipped catalog has instant weapons");
}

/// Spawns the shipped unit `slug` for `owner` at `(x, y)` holding position.
fn held(world: &mut SimWorld, slug: &str, owner: PlayerId, x: f64, y: f64) -> EntityId {
    let kind = world.catalog.id_by_slug(slug).expect("a shipped unit");
    let id = world
        .spawn_unit(kind, owner, at(x, y))
        .expect("a free spot");
    world
        .entities
        .get_mut(&id)
        .expect("spawned")
        .issue_order(UnitOrder::HoldPosition, false)
        .expect("hold admitted");
    id
}

/// Index of the ability named `name` in the archetype of `caster`.
fn ability_index(world: &SimWorld, caster: EntityId, name: &str) -> u8 {
    let kind = world.entities[&caster].unit_type_id;
    let arch = world.catalog.get(kind).expect("registered");
    let index = arch
        .abilities
        .iter()
        .position(|ability| ability.name == name)
        .expect("the caster has the ability");
    u8::try_from(index).expect("few abilities")
}

/// The id of the shipped status `name`.
fn status_id(world: &SimWorld, name: &str) -> sim_types::StatusId {
    world
        .catalog
        .spells()
        .status_id(name)
        .expect("a shipped status")
}

/// The statuses `snapshot` publishes for `entity`, in order.
fn listed(snapshot: &RenderSnapshot, entity: EntityId) -> Vec<sim_types::StatusId> {
    snapshot
        .statuses
        .iter()
        .filter(|view| view.entity == entity)
        .map(|view| view.status)
        .collect()
}

/// B-122 (plan `2026-11-17-effect-snapshot-identity` D-5): the frame lists each catalog status
/// of each published bubble, sorted by `(entity, status)`, and nothing for a unit carried in a
/// transport. A frame without statuses has no `statuses` key.
#[test]
fn statuses_list_each_status_of_each_published_unit_in_order() {
    let mut world = flat_world(0x0e_ff0c);
    world
        .research_upgrade(ATTACKER, "psionic_storm")
        .expect("the shipped research exists");
    let infestor = held(&mut world, "mycelium_shaman", ATTACKER, 7.5, 20.0);
    let templar = held(&mut world, "cerebral_elder", ATTACKER, 7.5, 40.0);
    for caster in [infestor, templar] {
        world.entities.get_mut(&caster).expect("spawned").energy = SimScalar::from_i32(200);
    }
    let fungaled = held(&mut world, "shock_golem", DEFENDER, 16.0, 20.0);
    let stormed = held(&mut world, "pulse_bot", DEFENDER, 16.0, 40.0);
    let passenger = held(&mut world, "pulse_bot", DEFENDER, 16.5, 40.5);
    let carrier = held(&mut world, "repair_shuttle", DEFENDER, 19.0, 44.0);

    let quiet = step(&mut world);
    assert_eq!(quiet.statuses, []);
    assert!(
        !ron::to_string(&quiet)
            .expect("a frame serializes")
            .contains("statuses"),
        "a frame without statuses has no key"
    );

    let fungal = ability_index(&world, infestor, "Fungal Growth");
    assert!(world.execute_ability(infestor, fungal, Some(at(16.0, 20.0)), None));
    let fungal_growth = status_id(&world, "fungal_growth");
    let mut expected = vec![fungal_growth, status_id(&world, "fungal_growth_slow")];
    expected.sort_unstable();
    (0..40)
        .find(|_| !listed(&step(&mut world), fungaled).is_empty())
        .expect("the missile lands within 40 ticks");
    let storm = ability_index(&world, templar, "Psionic Storm");
    assert!(world.execute_ability(templar, storm, Some(at(16.0, 40.0)), None));
    let psionic_storm = status_id(&world, "psionic_storm");
    (0..10)
        .find(|_| {
            step(&mut world);
            world.effects().has_status(passenger, psionic_storm)
        })
        .expect("the storm takes the passenger");
    world
        .try_load_unit_into_transport(passenger, carrier)
        .expect("the shuttle carries the stormed bot");
    assert!(
        !world.effects().statuses_of(passenger).is_empty(),
        "the carried unit keeps its status"
    );
    let frame = world.build_render_snapshot();

    assert_eq!(
        listed(&frame, fungaled),
        expected,
        "both of Fungal's statuses"
    );
    assert_eq!(listed(&frame, stormed), [psionic_storm]);
    assert_eq!(
        listed(&frame, passenger),
        [],
        "a carried unit is published as nothing"
    );
    assert!(
        frame.statuses.windows(2).all(|pair| pair[0] < pair[1]),
        "sorted by (entity, status), each once: {:?}",
        frame.statuses
    );
    for bubble in &frame.bubbles {
        let carried: Vec<_> = world
            .effects()
            .statuses_of(bubble.entity_id)
            .iter()
            .map(|active| active.status)
            .collect();
        assert_eq!(listed(&frame, bubble.entity_id), carried, "{bubble:?}");
    }
    assert!(frame
        .statuses
        .iter()
        .all(|view| frame.bubbles.iter().any(|b| b.entity_id == view.entity)));
    assert!(
        frame.statuses.contains(&StatusView {
            entity: fungaled,
            status: fungal_growth,
        }),
        "the identity tells Fungal Growth from Psionic Storm"
    );
}

/// The caster type and ability the scratch missile is launched under.
const SCRATCH_ABILITY: AbilitySource = AbilitySource {
    unit_type: UnitTypeId(3),
    ability: 1,
};

/// Installs spell tables adding a scratch unit-targeted missile presented as a projectile,
/// whose impact deals 1, and returns its root. Its names start with `scratch_` so they never
/// meet a shipped entry.
fn install_scratch_missile(world: &mut SimWorld) -> sim_types::EffectId {
    let statuses = "(statuses: [])";
    let effects = r#"(effects: [
        (name: "scratch_missile", node: LaunchMissile((to: TargetUnit, speed: 0.25,
            view: Projectile, impact: Use("scratch_missile_hit")))),
        (name: "scratch_missile_hit", node: Damage((amount: 1.0))),
    ])"#;
    let spells = sim_fixtures::spells_with(statuses, effects).expect("valid scratch tables");
    world
        .catalog
        .set_spell_catalog(spells)
        .expect("the shipped abilities resolve in the merged tables");
    world
        .catalog
        .spells()
        .effect_id("scratch_missile")
        .expect("installed")
}

/// Launches the scratch missile `root` from `from` at `target` for `ATTACKER`, with no caster.
fn launch_at(world: &mut SimWorld, root: sim_types::EffectId, from: SimVec2, target: EntityId) {
    world.run_effect(
        root,
        EffectContext {
            owner: ATTACKER,
            caster: None,
            ability: SCRATCH_ABILITY,
            origin: from,
            source_point: from,
            target: EffectTarget::Unit(target),
        },
    );
}

/// One published spell impact: where, which missile, whose and from which ability.
type Impact = ([f32; 3], u32, PlayerId, AbilitySource);

/// The spell impacts of `snapshot`, in order.
fn spell_impacts(snapshot: &RenderSnapshot) -> Vec<Impact> {
    snapshot
        .combat_events
        .iter()
        .filter_map(|event| match event {
            CombatEventVisual::SpellImpact {
                pos,
                missile,
                owner,
                source,
            } => Some((*pos, *missile, *owner, *source)),
            _ => None,
        })
        .collect()
}

/// B-124 (plan `2026-11-17-effect-snapshot-identity` D-6): a missile whose unit target dies in
/// flight is dropped without an impact, so it publishes none; the same missile at a living
/// target publishes one, on the tick it lands, at its contact point.
#[test]
fn a_missile_that_loses_its_target_publishes_no_impact() {
    for dies in [true, false] {
        let mut world = flat_world(0x0e_ff0d);
        let root = install_scratch_missile(&mut world);
        let target = held(&mut world, "shock_golem", DEFENDER, 26.0, 20.0);
        launch_at(&mut world, root, at(20.0, 20.0), target);
        assert_eq!(world.effects().missiles().len(), 1, "launched");
        let missile = world.effects().missiles()[0].id;
        step(&mut world);
        if dies {
            world.entities.get_mut(&target).expect("alive").hp = SimScalar::ZERO;
        }
        let mut impacts = Vec::new();
        for _ in 0..60 {
            let snapshot = step(&mut world);
            for event in spell_impacts(&snapshot) {
                impacts.push((snapshot.tick, event));
            }
            if world.effects().missiles().is_empty() {
                break;
            }
        }
        assert!(world.effects().missiles().is_empty(), "the missile is gone");
        if dies {
            assert_eq!(impacts, [], "a fizzled missile lays no impact");
            continue;
        }
        assert_eq!(impacts.len(), 1, "exactly one impact: {impacts:?}");
        let (tick, (pos, id, owner, source)) = impacts[0];
        assert_eq!(
            tick,
            Tick(world.current_tick),
            "published on the landing tick"
        );
        assert_eq!((id, owner, source), (missile, ATTACKER, SCRATCH_ABILITY));
        let radius = world
            .catalog
            .get(world.entities[&target].unit_type_id)
            .expect("registered")
            .collision_radius
            .to_f32();
        assert!(
            (pos[0] - (26.0 - radius)).abs() < 1e-4,
            "at the contact point: {pos:?}"
        );
        assert!((pos[1] - 20.0).abs() < 1e-4, "{pos:?}");
        assert!(pos[2].abs() < 1e-4, "on flat ground: {pos:?}");
    }
}
