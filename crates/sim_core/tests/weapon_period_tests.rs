//! Weapon timing arithmetic, actual firing, and explicit invalid-state evidence.
//!
//! Numeric boundary expectations are independently calculated from raw integers;
//! gameplay fixtures select real catalog entries by capabilities and test actual
//! weapon phases, damage, diagnostics and replay through public APIs.

use sim_core::weapon_timing::{combined_attack_rate, resolve_attack_period};
use sim_core::{ActiveBuff, AttackRate, SimWorld, WeaponPhase};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, OverclockSpec, PlayerId,
    SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitStatDeltas,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn buff(rate: SimScalar) -> ActiveBuff {
    ActiveBuff {
        buff_id: 42,
        remaining_ticks: 500,
        speed_multiplier: SimScalar::ONE,
        attack_rate_multiplier: rate,
    }
}

#[test]
fn intervals_match_integer_reference_at_signed_and_rate_boundaries() -> TestResult {
    for base in [0_u16, 1, 23, 60_000, u16::MAX] {
        for delta in [i32::MIN, -65_536, -1, 0, 1, 60_000, i32::MAX] {
            for raw in [1_i64, 1 << 31, 1 << 32, 3 << 31, 2 << 32, i64::MAX] {
                let rate = AttackRate::new(SimScalar::from_raw(raw))?;
                let mathematical =
                    (i128::from(base) + i128::from(delta)) * 4_294_967_296 / i128::from(raw);
                let expected = if mathematical < 1 {
                    1
                } else if mathematical > 65_535 {
                    65_535
                } else {
                    u16::try_from(mathematical)?
                };
                assert_eq!(resolve_attack_period(base, delta, rate), expected);
            }
        }
    }
    let twice = AttackRate::new(SimScalar::from_i32(2))?;
    assert_eq!(
        resolve_attack_period(60_000, 60_000, twice),
        60_000,
        "must not clamp before scaling"
    );
    assert_eq!(
        resolve_attack_period(1, -1, AttackRate::new(SimScalar::HALF)?),
        1,
        "must not apply a one-tick minimum before scaling"
    );
    Ok(())
}

#[test]
fn positive_rate_fold_preserves_defined_precision_and_order() -> TestResult {
    let fold = |rates: &[SimScalar]| {
        combined_attack_rate(&rates.iter().copied().map(buff).collect::<Vec<_>>())
    };
    assert_eq!(fold(&[])?, AttackRate::ONE);
    assert_eq!(
        fold(&[SimScalar::from_num(1.5)])?.value(),
        SimScalar::from_num(1.5)
    );
    assert_eq!(
        fold(&[SimScalar::from_i32(2), SimScalar::HALF])?,
        AttackRate::ONE
    );
    assert_eq!(
        fold(&[SimScalar::HALF, SimScalar::HALF, SimScalar::from_i32(4)])?,
        AttackRate::ONE
    );
    let epsilon = SimScalar::from_raw(1);
    assert_eq!(fold(&[epsilon, epsilon])?.value(), epsilon);
    assert_eq!(
        fold(&[SimScalar::MAX, SimScalar::from_i32(2)])?.value(),
        SimScalar::MAX
    );
    let saturated_then_half =
        fold(&[SimScalar::MAX, SimScalar::from_i32(2), SimScalar::HALF])?.value();
    assert_eq!(saturated_then_half.raw(), i64::MAX / 2);
    let unsaturated_order =
        fold(&[SimScalar::from_i32(2), SimScalar::HALF, SimScalar::MAX])?.value();
    assert_eq!(unsaturated_order, SimScalar::MAX);
    assert_ne!(
        saturated_then_half, unsaturated_order,
        "ordered quantization is not associative"
    );
    for invalid in [SimScalar::ZERO, -SimScalar::ONE] {
        assert!(AttackRate::new(invalid).is_err());
        let error = fold(&[SimScalar::ONE, invalid])
            .err()
            .ok_or("invalid rate accepted")?;
        assert_eq!(error.buff_index, 1);
        assert_eq!(error.buff_id, 42);
        assert_eq!(error.source.raw, invalid.raw());
    }
    Ok(())
}

fn battle(
    base: u16,
    delta: i32,
    rate: SimScalar,
) -> Result<(SimWorld, EntityId, EntityId, u16), Box<dyn std::error::Error>> {
    let map = GameMap::new(16, 16, MapCell::default_ground(CliffLevel(0)))?;
    let mut world = SimWorld::new(map, 123);
    let unit_type = world
        .catalog
        .archetypes()
        .find(|a| a.weapon.is_some() && a.max_speed > SimScalar::ZERO)
        .ok_or("real catalog has no mobile weapon")?
        .unit_type_id;
    let arch = world.catalog.get_mut(unit_type)?;
    let mut weapon = arch.weapon.clone().ok_or("weapon missing")?;
    weapon.period_ticks = base;
    weapon.min_range = SimScalar::ZERO;
    weapon.max_range = SimScalar::from_i32(10);
    weapon.projectile_speed = None;
    weapon.damage_point_ticks = 1;
    weapon.backswing_ticks = 0;
    let damage_point = weapon.damage_point_ticks;
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon);
    let slug = arch.slug.clone();
    let attacker = world.spawn_unit(
        unit_type,
        PlayerId(0),
        SimVec2::new(SimScalar::from_i32(5), SimScalar::from_i32(5)),
    )?;
    let target = world.spawn_unit(
        unit_type,
        PlayerId(1),
        SimVec2::new(SimScalar::from_i32(7), SimScalar::from_i32(5)),
    )?;
    let durable_target = world.entities.get_mut(&target).ok_or("target missing")?;
    durable_target.max_hp = SimScalar::from_i32(10_000);
    durable_target.hp = durable_target.max_hp;
    world
        .player_upgrade_state_mut(PlayerId(0))
        .unit_stat_modifiers
        .insert(
            slug,
            UnitStatDeltas {
                attack_cooldown_delta: delta,
                ..Default::default()
            },
        );
    let entity = world
        .entities
        .get_mut(&attacker)
        .ok_or("attacker missing")?;
    entity
        .order_queue
        .push_back(UnitOrder::AttackTarget { target });
    entity.weapon_phase = WeaponPhase::Windup {
        target_id: target,
        remaining_ticks: 1,
    };
    entity.active_buffs.push(buff(rate));
    Ok((world, attacker, target, damage_point))
}

#[test]
fn actual_firing_uses_full_modified_interval_and_reports_invalid_buffs() -> TestResult {
    for (base, delta, rate, expected) in [
        (30, 0, SimScalar::from_num(1.5), 20_u16),
        (60_000, 60_000, SimScalar::from_i32(2), 60_000),
        (30, 0, SimScalar::from_raw(1), u16::MAX),
    ] {
        let (mut world, attacker, target, damage_point) = battle(base, delta, rate)?;
        let before = world.entities[&target].hp + world.entities[&target].shields;
        world.step_weapons_and_projectiles();
        assert_eq!(world.diagnostics(), []);
        assert_eq!(
            world.entities[&attacker].weapon_cooldown_remaining,
            expected - damage_point
        );
        assert!(world.entities[&target].hp + world.entities[&target].shields < before);
        // Explicit near-completion cooldown fixture exercises the real transition.
        world
            .entities
            .get_mut(&attacker)
            .ok_or("attacker missing")?
            .weapon_cooldown_remaining = 1;
        for _ in 0..4 {
            world.step_weapons_and_projectiles();
        }
        assert!(world.entities[&attacker].weapon_cooldown_remaining > 1);
    }
    let (mut world, attacker, target, _) = battle(30, 0, SimScalar::ZERO)?;
    let before = world.entities[&target].clone();
    world.step_weapons_and_projectiles();
    assert_eq!(world.entities[&target].hp, before.hp);
    assert_eq!(world.entities[&attacker].weapon_cooldown_remaining, 0);
    assert!(
        matches!(world.diagnostics(), [sim_core::SimulationDiagnostic::InvalidBuffAttackRate {
        entity, source, ..
    }] if *entity == attacker && source.source.raw == 0)
    );
    let digest = world.compute_checksum();
    assert_eq!(world.drain_diagnostics().count(), 1);
    assert_eq!(
        world.compute_checksum(),
        digest,
        "diagnostics are derived outputs"
    );
    world
        .entities
        .get_mut(&attacker)
        .ok_or("attacker missing")?
        .active_buffs
        .clear();
    world.step(&TickBundle {
        tick: Tick(1),
        commands: vec![],
        state_checksum: None,
    });
    assert_eq!(world.diagnostics(), []);
    Ok(())
}

#[test]
fn invalid_overclock_admission_preserves_costs_cooldowns_and_existing_buffs() -> TestResult {
    let (mut world, attacker, _, _) = battle(30, 0, SimScalar::ONE)?;
    for rate in [SimScalar::ZERO, -SimScalar::ONE] {
        let spec = OverclockSpec {
            hp_cost: SimScalar::ONE,
            duration_ticks: 100,
            speed_multiplier: SimScalar::ONE,
            attack_rate_multiplier: rate,
        };
        let entity = world
            .entities
            .get_mut(&attacker)
            .ok_or("attacker missing")?;
        entity.energy = SimScalar::from_i32(10);
        let before = entity.clone();
        assert_eq!(
            entity.apply_overclock(spec),
            Err(sim_core::entity::EntityAdmissionError::AttackRate(
                sim_core::weapon_timing::InvalidAttackRate { raw: rate.raw() }
            ))
        );
        assert_eq!(*entity, before);
        let archetype = world.catalog.get_mut(before.unit_type_id)?;
        archetype.abilities = vec![AbilitySpec {
            name: "invalid-rate-fixture".into(),
            target_kind: AbilityTargetKind::InstantSelf,
            energy_cost: SimScalar::ONE,
            cooldown_ticks: 20,
            cast_range: SimScalar::ZERO,
            effect: AbilityEffectSpec::Overclock(spec),
        }];
        assert!(!world.execute_ability(attacker, 0, None, None));
        assert_eq!(world.entities[&attacker], before);
    }
    Ok(())
}

#[test]
fn modifier_replay_matches_full_state_and_digest() -> TestResult {
    let (mut first, _, _, _) = battle(60_000, 60_000, SimScalar::from_i32(2))?;
    let (mut second, _, _, _) = battle(60_000, 60_000, SimScalar::from_i32(2))?;
    for tick in 1..65 {
        let bundle = TickBundle {
            tick: Tick(tick),
            commands: vec![],
            state_checksum: None,
        };
        first.step(&bundle);
        second.step(&bundle);
        assert_eq!(first.entities, second.entities);
        assert_eq!(first.compute_checksum(), second.compute_checksum());
        assert_eq!(first.diagnostics(), []);
    }
    Ok(())
}
