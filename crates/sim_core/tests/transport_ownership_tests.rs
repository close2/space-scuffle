//! Public carrier transactions and real tick lifecycle with authored catalog fixtures.
//! Tests preserve captured economy, inspect exact admission failures and exercise
//! cargo absence without inventing capacity, boarding range or transport identities.

use sim_core::catalog::RuntimeCatalogs;
use sim_core::movement::{DisplacementAdmissionError, MovementAdmissionError};
use sim_core::production::ProductionAdmissionError;
use sim_core::transport::{TransportAction, TransportAdmissionError};
#[path = "support/scratch_storm.rs"]
mod scratch_storm;

use scratch_storm::{open_storm, ScratchStorm};
use sim_core::{SimWorld, UnitCatalog, UpgradeCatalog, WeaponPhase, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, AnimState, CombatEventVisual, EntityId,
    MoverType, PlayerCommand, PlayerId, PlayerSlot, ShapeKind, SimScalar, SimVec2, Tick,
    TickBundle, UnitModeSpec, UnitOrder, UnitTypeId, VisibilityState,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
struct Fixture {
    world: SimWorld,
    kind: UnitTypeId,
    weapon: WeaponSpec,
}
impl Fixture {
    fn new() -> TestResult<Self> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog");
        let mut catalog: UnitCatalog = RuntimeCatalogs::load_from_directory(root)?.units;
        let kind = catalog
            .archetypes()
            .find(|a| {
                a.mover_type == MoverType::Ground
                    && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                    && !a.is_immobile_by_default()
                    && a.weapon.is_some()
            })
            .ok_or("mobile armed catalog fixture missing")?
            .unit_type_id;
        let arch = catalog.get_mut(kind)?;
        let weapon = arch.weapon.clone().ok_or("weapon missing")?;
        arch.weapon = None;
        arch.ground_weapon = None;
        arch.air_weapon = None;
        arch.collision_radius = SimScalar::HALF;
        arch.max_hp = SimScalar::from_num(100);
        arch.max_shields = SimScalar::ZERO;
        arch.max_energy = SimScalar::from_num(100);
        arch.hp_regen_per_tick = SimScalar::ONE;
        arch.energy_regen_per_tick = SimScalar::ONE;
        arch.sight_radius = SimScalar::from_num(3);
        arch.is_detector = true;
        arch.scan_range = SimScalar::ZERO;
        arch.supply_cost = 2;
        arch.supply_provided = 80;
        arch.mineral_cost = 7;
        arch.gas_cost = 3;
        arch.build_time_ticks = 100;
        arch.requires_structures.clear();
        arch.trains_units = vec![arch.slug.clone()];
        arch.abilities = vec![
            ability(AbilityEffectSpec::SiegeModeToggle {
                cast_ticks: 3,
                sieged_weapon: Box::new(weapon.clone()),
            }),
            ability(AbilityEffectSpec::SwitchMode {
                mode_name: "transport-fixture-mode".into(),
            }),
        ];
        arch.modes = vec![UnitModeSpec {
            mode_name: "transport-fixture-mode".into(),
            cast_ticks: 3,
            leave_cast_ticks: 3,
            cast_random_delay_max_ticks: SimScalar::ZERO,
            leave_random_delay_max_ticks: SimScalar::ZERO,
            is_immobile: true,
            unpushable: true,
            is_cloaked: false,
            burrowed: false,
            mover_type_override: None,
            sight_radius_override: None,
            ground_weapon_override: None,
            air_weapon_override: None,
            energy_cost_per_second: SimScalar::ZERO,
        }];
        let map = GameMap::new(40, 40, MapCell::default_ground(CliffLevel(0)))?;
        Ok(Self {
            world: SimWorld::with_catalogs(map, 51, catalog, UpgradeCatalog::default()),
            kind,
            weapon,
        })
    }
    /// Spawns the fixture unit with a full energy pool, so its abilities can be cast at once.
    fn spawn(&mut self, owner: u8, x: i32, y: i32) -> TestResult<EntityId> {
        let id = self
            .world
            .spawn_unit(self.kind, PlayerId(owner), point(x, y))?;
        let max_energy = self.world.catalog.get(self.kind)?.max_energy;
        // Rule fixture: the pool is filled directly, whatever the archetype starts with.
        self.world.entities.get_mut(&id).ok_or("spawned")?.energy = max_energy;
        Ok(id)
    }
    fn tick(&mut self, commands: Vec<PlayerCommand>) {
        self.world.step(&TickBundle {
            tick: Tick(self.world.current_tick),
            commands,
            state_checksum: None,
        });
    }
}
fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
}
fn ability(effect: AbilityEffectSpec) -> AbilitySpec {
    AbilitySpec {
        name: "transport test channel".into(),
        target_kind: AbilityTargetKind::InstantSelf,
        energy_cost: SimScalar::from_num(5),
        cooldown_ticks: 8,
        cast_range: SimScalar::ZERO,
        effect,
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }
}
fn command(actor: EntityId, order: UnitOrder) -> PlayerCommand {
    PlayerCommand {
        player_id: PlayerId(0),
        entities: vec![actor],
        order,
        queue: false,
    }
}

#[test]
fn exact_link_commit_retains_passive_weapon_and_economic_state() -> TestResult {
    let mut f = Fixture::new()?;
    let passenger = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    let actor = f.world.entities.get_mut(&passenger).ok_or("passenger")?;
    actor.issue_order(
        UnitOrder::Move {
            target: point(7, 4),
        },
        false,
    )?;
    actor.current_target = Some(carrier);
    actor.weapon_phase = WeaponPhase::Windup {
        target_id: carrier,
        remaining_ticks: 2,
    };
    actor.weapon_cooldown_remaining = 9;
    actor.current_anim = AnimState::Attack;
    actor.current_funnel_waypoints.push(point(6, 4));
    actor.velocity = point(1, 0);
    let before = actor.clone();
    let economy = f.world.player_resources;
    let next_id = f.world.next_entity_id;
    let receipt = f.world.try_load_unit_into_transport(passenger, carrier)?;
    assert_eq!(receipt.action, TransportAction::Load);
    let after = &f.world.entities[&passenger];
    assert_eq!(after.transport_carrier(), Some(carrier));
    assert!(after.order_queue.is_empty() && after.current_funnel_waypoints.is_empty());
    assert_eq!(after.current_target, None);
    assert_eq!(after.velocity, SimVec2::ZERO);
    assert_eq!(after.position, before.position);
    assert_eq!(after.pos_at_tick_start, before.pos_at_tick_start);
    assert_eq!(after.weapon_phase, before.weapon_phase);
    assert_eq!(
        after.weapon_cooldown_remaining,
        before.weapon_cooldown_remaining
    );
    assert_eq!(after.current_anim, before.current_anim);
    assert_eq!(f.world.player_resources, economy);
    assert_eq!(f.world.next_entity_id, next_id);
    assert!(f.world.has_transport_passengers(carrier)?);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn invalid_requests_and_exact_occupied_unload_preserve_world() -> TestResult {
    let mut f = Fixture::new()?;
    let passenger = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    let foreign = f.spawn(1, 32, 32)?;
    for (source, target, expected) in [
        (
            passenger,
            EntityId(u32::MAX),
            TransportAdmissionError::MissingEntity(EntityId(u32::MAX)),
        ),
        (
            passenger,
            passenger,
            TransportAdmissionError::SameEntity(passenger),
        ),
        (
            passenger,
            foreign,
            TransportAdmissionError::DifferentOwners {
                passenger,
                carrier: foreign,
            },
        ),
    ] {
        let before = f.world.compute_checksum();
        assert_eq!(
            f.world.try_load_unit_into_transport(source, target),
            Err(expected)
        );
        assert_eq!(f.world.compute_checksum(), before);
    }
    f.world.try_load_unit_into_transport(passenger, carrier)?;
    let before = f.world.compute_checksum();
    assert_eq!(
        f.world.try_load_unit_into_transport(carrier, foreign),
        Err(TransportAdmissionError::DifferentOwners {
            passenger: carrier,
            carrier: foreign
        })
    );
    assert_eq!(
        f.world.try_load_unit_into_transport(carrier, passenger),
        Err(TransportAdmissionError::AlreadyLoaded(passenger))
    );
    assert_eq!(
        f.world
            .try_unload_unit_from_transport(passenger, point(24, 24)),
        Err(TransportAdmissionError::Destination(
            DisplacementAdmissionError::Occupied(carrier)
        ))
    );
    assert_eq!(f.world.compute_checksum(), before);
    f.world
        .try_unload_unit_from_transport(passenger, point(25, 24))?; // exact tangency
    assert_eq!(f.world.entities[&passenger].position, point(25, 24));
    assert!(!f.world.has_transport_passengers(carrier)?);
    Ok(())
}

#[test]
fn passenger_commands_are_rejected_but_carrier_work_and_passive_timers_continue() -> TestResult {
    for ability_index in [0, 1] {
        let mut f = Fixture::new()?;
        let passenger = f.spawn(0, 4, 4)?;
        let carrier = f.spawn(0, 24, 24)?;
        f.world
            .try_queue_production(PlayerId(0), &[carrier], f.kind)?;
        assert!(f.world.execute_ability(carrier, ability_index, None, None));
        f.world.try_load_unit_into_transport(passenger, carrier)?;
        let actor = f.world.entities.get_mut(&passenger).ok_or("passenger")?;
        actor.hp = SimScalar::from_num(50);
        actor.energy = SimScalar::from_num(50);
        actor.ability_cooldowns[0] = 2;
        actor.weapon_cooldown_remaining = 9;
        let before = actor.clone();
        assert!(!f
            .world
            .execute_ability(passenger, ability_index, None, None));
        assert_eq!(f.world.entities[&passenger], before);
        let kind = f.kind;
        f.tick(vec![
            command(passenger, UnitOrder::Produce { unit_type: kind }),
            command(
                passenger,
                UnitOrder::Move {
                    target: point(8, 4),
                },
            ),
            command(
                passenger,
                UnitOrder::CastAbility {
                    ability_index,
                    target_pos: None,
                    target_entity: None,
                },
            ),
        ]);
        assert_eq!(
            f.world.production_outcomes()[0].result,
            Err(ProductionAdmissionError::LoadedRecipient(passenger))
        );
        assert_eq!(
            f.world.movement_outcomes()[0].result,
            Err(MovementAdmissionError::LoadedRecipient(passenger))
        );
        let actor = &f.world.entities[&passenger];
        assert!(actor.production_queue.is_empty() && actor.order_queue.is_empty());
        assert_eq!(actor.channeling_remaining_ticks, 0);
        assert_eq!(actor.pending_ability, None);
        assert_eq!(actor.hp, SimScalar::from_num(51));
        assert_eq!(actor.energy, SimScalar::from_num(51));
        assert_eq!(actor.ability_cooldowns[0], 1);
        assert_eq!(actor.weapon_cooldown_remaining, 9);
        assert_eq!(f.world.entities[&carrier].channeling_remaining_ticks, 2);
        assert_eq!(
            f.world.entities[&carrier].production_queue[0].remaining_ticks,
            99
        );
        assert_eq!(f.world.diagnostics(), []);
    }
    Ok(())
}

#[test]
fn busy_passenger_loading_does_not_cancel_or_refund_paid_work() -> TestResult {
    for paid in [true, false] {
        let mut f = Fixture::new()?;
        let passenger = f.spawn(0, 4, 4)?;
        let carrier = f.spawn(0, 24, 24)?;
        if paid {
            f.world
                .try_queue_production(PlayerId(0), &[passenger], f.kind)?;
        } else {
            assert!(f.world.execute_ability(passenger, 0, None, None));
        }
        let before = f.world.compute_checksum();
        assert_eq!(
            f.world.try_load_unit_into_transport(passenger, carrier),
            Err(TransportAdmissionError::Busy(passenger))
        );
        assert_eq!(f.world.compute_checksum(), before);
    }
    Ok(())
}

#[test]
fn cargo_has_no_vision_storm_presence_or_snapshot_but_returns_at_exact_endpoint() -> TestResult {
    let mut f = Fixture::new()?;
    let passenger = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    f.world.update_vision();
    assert_eq!(
        f.world.vision_maps[PlayerSlot::ALL[0]].visibility_at(4, 4),
        VisibilityState::Visible
    );
    f.world.try_load_unit_into_transport(passenger, carrier)?;
    // A storm on the passenger's spot finds nothing: a carried unit is hidden (plan
    // `2026-11-10-spellcaster-mechanics` §7; the hazard zone's replacement, D-12).
    ScratchStorm {
        radius: "2.0",
        damage: "30.0",
        structures: false,
    }
    .install(&mut f.world);
    open_storm(&mut f.world, PlayerId(1), point(4, 4));
    f.tick(vec![]);
    assert_eq!(f.world.effects().statuses_of(passenger).len(), 0);
    assert_ne!(
        f.world.vision_maps[PlayerSlot::ALL[0]].visibility_at(4, 4),
        VisibilityState::Visible
    );
    assert!(!f.world.vision_maps[PlayerSlot::ALL[0]].is_detected(4, 4));
    assert_eq!(f.world.entities[&passenger].hp, SimScalar::from_num(100));
    let snapshot = f.world.build_render_snapshot();
    assert!(!snapshot.bubbles.iter().any(|b| b.entity_id == passenger));
    assert!(snapshot.bubbles.iter().any(|b| b.entity_id == carrier));
    f.world
        .try_unload_unit_from_transport(passenger, point(10, 4))?;
    assert!(f
        .world
        .build_render_snapshot()
        .bubbles
        .iter()
        .any(|b| b.entity_id == passenger && b.world_pos[..2] == [10.0, 4.0]));
    Ok(())
}

#[test]
fn carrier_death_releases_cumulative_supply_and_jobs_once_in_id_order() -> TestResult {
    let mut f = Fixture::new()?;
    let low = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    let high = f.spawn(0, 8, 4)?;
    let survivor = f.spawn(0, 32, 32)?;
    f.world
        .try_queue_production(PlayerId(0), &[carrier], f.kind)?;
    f.world.try_load_unit_into_transport(low, carrier)?;
    f.world.try_load_unit_into_transport(high, carrier)?;
    assert_eq!(
        f.world.player_resources[PlayerSlot::ALL[0]].supply_provided_total,
        320
    );
    f.world.entities.get_mut(&carrier).ok_or("carrier")?.hp = SimScalar::ZERO;
    f.world.entities.get_mut(&low).ok_or("passenger")?.hp = SimScalar::ZERO;
    assert!(
        matches!(f.world.try_unload_unit_from_transport(high, point(12, 4)), Err(TransportAdmissionError::DeadEntity(id)) if id == carrier)
    );
    f.world.reap_dead_and_emit_shatter();
    assert_eq!(
        f.world.entities.keys().copied().collect::<Vec<_>>(),
        [survivor]
    );
    assert_eq!(f.world.player_resources[PlayerSlot::ALL[0]].supply_used, 2);
    assert_eq!(
        f.world.player_resources[PlayerSlot::ALL[0]].supply_provided_total,
        80
    );
    assert_eq!(f.world.player_resources[PlayerSlot::ALL[0]].supply_max, 80);
    f.world.validate_supply_ledger()?;
    assert_eq!(f.world.pending_events.len(), 1);
    assert!(
        matches!(f.world.pending_events[0], CombatEventVisual::BubbleShatter { pos, owner: PlayerId(0), .. } if pos[..2] == [24.0, 24.0])
    );
    f.world.reap_dead_and_emit_shatter();
    assert_eq!(f.world.pending_events.len(), 1);
    Ok(())
}

#[test]
fn independently_dead_passenger_leaves_carrier_and_emits_no_pickup_shatter() -> TestResult {
    let mut f = Fixture::new()?;
    let passenger = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    f.world.try_load_unit_into_transport(passenger, carrier)?;
    f.world.entities.get_mut(&passenger).ok_or("passenger")?.hp = SimScalar::ZERO;
    f.world.reap_dead_and_emit_shatter();
    assert!(f.world.entities.contains_key(&carrier));
    assert!(!f.world.has_transport_passengers(carrier)?);
    assert_eq!(f.world.pending_events, []);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn public_entity_helpers_do_not_advance_cargo_weapon_or_apply_push() -> TestResult {
    let mut f = Fixture::new()?;
    let passenger = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    f.world.try_load_unit_into_transport(passenger, carrier)?;
    let mut actor = f.world.entities[&passenger].clone();
    let mut other = f.world.entities[&carrier].clone();
    actor.weapon_phase = WeaponPhase::Windup {
        target_id: carrier,
        remaining_ticks: 1,
    };
    actor.weapon_cooldown_remaining = 7;
    let before = actor.clone();
    let other_before = other.clone();
    assert!(!sim_core::step_entity_weapon(
        &mut actor,
        &f.weapon,
        Some(sim_core::WeaponTarget {
            id: other.id,
            position: point(6, 4),
            combined_radius: SimScalar::HALF,
        })
    ));
    assert!(!actor.issue_order(UnitOrder::Stop, false)?);
    sim_core::resolve_push_interaction(
        &mut actor,
        &mut other,
        SimScalar::from_num(-1),
        SimScalar::from_num(-1),
    )?;
    assert_eq!(actor, before);
    assert_eq!(other, other_before);
    assert_eq!(
        f.world
            .compute_entity_flee_destination(passenger, point(5, 4), SimScalar::ONE)?,
        None
    );
    Ok(())
}

#[test]
fn carrier_identity_changes_checksum_and_schedule_replays_for_two_thousand_ticks() -> TestResult {
    let mut left = Fixture::new()?;
    let mut right = Fixture::new()?;
    let mut ids = Vec::new();
    for f in [&mut left, &mut right] {
        ids.push([f.spawn(0, 4, 4)?, f.spawn(0, 24, 24)?, f.spawn(0, 32, 32)?]);
    }
    left.world
        .try_load_unit_into_transport(ids[0][0], ids[0][1])?;
    right
        .world
        .try_load_unit_into_transport(ids[1][0], ids[1][2])?;
    assert_ne!(
        left.world.compute_checksum(),
        right.world.compute_checksum()
    );
    right
        .world
        .try_unload_unit_from_transport(ids[1][0], point(4, 4))?;
    right
        .world
        .try_load_unit_into_transport(ids[1][0], ids[1][1])?;
    for tick in 0..2000 {
        for (f, ids) in [(&mut left, ids[0]), (&mut right, ids[1])] {
            if tick % 100 == 25 {
                f.world
                    .try_unload_unit_from_transport(ids[0], point(4, 4))?;
            }
            if tick % 100 == 75 {
                f.world.try_load_unit_into_transport(ids[0], ids[1])?;
            }
            if tick == 1999 {
                f.world.entities.get_mut(&ids[1]).ok_or("carrier")?.hp = SimScalar::ZERO;
            }
            f.tick(vec![]);
            assert_eq!(f.world.diagnostics(), []);
        }
        assert_eq!(
            left.world.compute_checksum(),
            right.world.compute_checksum()
        );
    }
    Ok(())
}

#[test]
fn loaded_ground_and_air_targets_preserve_shooter_windup_without_aim_or_damage() -> TestResult {
    for mover in [MoverType::Ground, MoverType::Air] {
        let mut f = Fixture::new()?;
        let target_kind = if mover == MoverType::Ground {
            f.kind
        } else {
            f.world
                .catalog
                .archetypes()
                .find(|a| {
                    a.mover_type == MoverType::Air
                        && !matches!(a.shape, ShapeKind::BuildingDome { .. })
                })
                .ok_or("Air catalog fixture missing")?
                .unit_type_id
        };
        let mut weapon = f.weapon.clone();
        weapon.projectile_speed = None;
        weapon.period_ticks = 10;
        weapon.damage_point_ticks = 2;
        weapon.backswing_ticks = 0;
        weapon.has_turret = true;
        weapon.max_range = SimScalar::from_num(20);
        weapon.visual_kind = sim_types::WeaponVisualKind::Hitscan;
        let source = f.world.catalog.get_mut(f.kind)?;
        if mover == MoverType::Air {
            source.air_weapon = Some(weapon);
        } else {
            source.ground_weapon = Some(weapon);
        }
        let shooter = f.spawn(0, 4, 4)?;
        let target = f.world.spawn_unit(target_kind, PlayerId(1), point(8, 4))?;
        let carrier = f.spawn(1, 24, 24)?;
        let actor = f.world.entities.get_mut(&shooter).ok_or("shooter")?;
        actor.issue_order(UnitOrder::attack(target), false)?;
        actor.weapon_phase = WeaponPhase::Windup {
            target_id: target,
            remaining_ticks: 1,
        };
        let original_aim = actor.turret_angle;
        let before = (
            f.world.entities[&target].hp,
            f.world.entities[&target].shields,
        );
        f.world.try_load_unit_into_transport(target, carrier)?;
        f.world.step_weapons_and_projectiles();
        // Losing the target cancels the swing at no cost (S-12).
        assert_eq!(f.world.entities[&shooter].weapon_phase, WeaponPhase::Ready);
        assert_eq!(f.world.entities[&shooter].weapon_cooldown_remaining, 0);
        assert_eq!(f.world.entities[&shooter].turret_angle, original_aim);
        assert_eq!(
            (
                f.world.entities[&target].hp,
                f.world.entities[&target].shields
            ),
            before
        );
        assert!(f.world.projectiles.is_empty() && f.world.pending_events.is_empty());
        f.world
            .try_unload_unit_from_transport(target, point(8, 4))?;
        f.world
            .entities
            .get_mut(&shooter)
            .ok_or("shooter")?
            .weapon_phase = WeaponPhase::Windup {
            target_id: target,
            remaining_ticks: 1,
        };
        f.world.step_weapons_and_projectiles();
        assert!(
            f.world.entities[&target].hp < before.0 || f.world.entities[&target].shields < before.1
        );
        assert!(f.world.pending_events.iter().any(|event| matches!(
            event,
            CombatEventVisual::HitscanTracer {
                owner: PlayerId(0),
                ..
            }
        )));
    }
    Ok(())
}

#[test]
fn loaded_shooter_retains_windup_then_resumes_without_stale_target_after_unload() -> TestResult {
    let mut f = Fixture::new()?;
    let mut weapon = f.weapon.clone();
    weapon.period_ticks = 10;
    weapon.damage_point_ticks = 2;
    weapon.backswing_ticks = 0;
    weapon.max_range = SimScalar::from_num(20);
    f.world.catalog.get_mut(f.kind)?.ground_weapon = Some(weapon);
    let shooter = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 24, 24)?;
    let target = f.spawn(1, 8, 4)?;
    let actor = f.world.entities.get_mut(&shooter).ok_or("shooter")?;
    actor.issue_order(UnitOrder::attack(target), false)?;
    actor.weapon_phase = WeaponPhase::Windup {
        target_id: target,
        remaining_ticks: 1,
    };
    actor.weapon_cooldown_remaining = 4;
    f.world.try_load_unit_into_transport(shooter, carrier)?;
    for _ in 0..3 {
        f.world.step_weapons_and_projectiles();
    }
    assert_eq!(
        f.world.entities[&shooter].weapon_phase,
        WeaponPhase::Windup {
            target_id: target,
            remaining_ticks: 1
        }
    );
    assert_eq!(f.world.entities[&shooter].weapon_cooldown_remaining, 4);
    let target_hp = f.world.entities[&target].hp;
    f.world
        .try_unload_unit_from_transport(shooter, point(4, 4))?;
    f.world.step_weapons_and_projectiles();
    assert_eq!(f.world.entities[&shooter].weapon_phase, WeaponPhase::Ready);
    assert_eq!(f.world.entities[&shooter].weapon_cooldown_remaining, 3);
    assert_eq!(f.world.entities[&target].hp, target_hp);
    assert!(f.world.projectiles.is_empty() && f.world.pending_events.is_empty());
    Ok(())
}

#[test]
fn local_foundation_failure_keeps_live_cargo_and_allows_unrelated_group_reap() -> TestResult {
    let mut f = Fixture::new()?;
    let passenger = f.spawn(0, 4, 4)?;
    let dome_kind = f
        .world
        .catalog
        .archetypes()
        .find(|a| matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .ok_or("foundation catalog fixture missing")?
        .unit_type_id;
    let carrier = f.world.spawn_structure(dome_kind, PlayerId(0), 20, 20)?;
    let unrelated = f.spawn(0, 32, 32)?;
    f.world.try_load_unit_into_transport(passenger, carrier)?;
    f.world.entities.get_mut(&carrier).ok_or("carrier")?.hp = SimScalar::ZERO;
    f.world.entities.get_mut(&unrelated).ok_or("unrelated")?.hp = SimScalar::ZERO;
    let passenger_before = f.world.entities[&passenger].clone();
    let original_cell = f.world.map.cell(20, 20)?.clone();
    f.world.map.cell_mut(20, 20)?.occupying_structure = None;
    f.world.reap_dead_and_emit_shatter();
    assert_eq!(f.world.entities[&passenger], passenger_before);
    assert!(f.world.entities.contains_key(&carrier));
    assert!(!f.world.entities.contains_key(&unrelated));
    assert_eq!(f.world.pending_events.len(), 1);
    assert!(matches!(
        f.world.diagnostics(),
        [sim_core::SimulationDiagnostic::ProductionInvariant { .. }]
    ));
    f.world.validate_supply_ledger()?;
    *f.world.map.cell_mut(20, 20)? = original_cell;
    f.world.reap_dead_and_emit_shatter();
    assert_eq!(f.world.entities, std::collections::BTreeMap::new());
    assert_eq!(f.world.player_resources[PlayerSlot::ALL[0]].supply_used, 0);
    assert_eq!(
        f.world.player_resources[PlayerSlot::ALL[0]].supply_provided_total,
        0
    );
    assert_eq!(f.world.pending_events.len(), 2);
    f.world.validate_supply_ledger()?;
    Ok(())
}

#[test]
fn all_three_splash_routes_ignore_cargo_at_dormant_pickup_coordinates() -> TestResult {
    for route in 0..3 {
        let mut f = Fixture::new()?;
        f.world.catalog.get_mut(f.kind)?.sight_radius = SimScalar::from_num(20);
        let shooter = f.spawn(0, 4, 4)?;
        let target = f.spawn(1, 9, 4)?;
        let passenger = f.spawn(1, 8, 4)?;
        let carrier = f.spawn(1, 30, 30)?;
        let mut weapon = f.weapon.clone();
        weapon.projectile_speed = None;
        weapon.suicide_on_attack = false;
        weapon.max_range = SimScalar::from_num(20);
        weapon.splash = Some(sim_types::SplashSpec {
            zones: vec![
                sim_types::SplashZone {
                    radius: SimScalar::from_num(2),
                    fraction: SimScalar::ONE,
                },
                sim_types::SplashZone {
                    radius: SimScalar::from_num(3),
                    fraction: SimScalar::HALF,
                },
                sim_types::SplashZone {
                    radius: SimScalar::from_num(4),
                    fraction: SimScalar::from_num(0.25),
                },
            ],
            shape: sim_types::SplashShape::Circle,
            friendly_fire: true,
            offset_by_unit_radius: false,
        });
        if route == 0 {
            f.world.catalog.get_mut(f.kind)?.ground_weapon = Some(weapon.clone());
            let actor = f.world.entities.get_mut(&shooter).ok_or("shooter")?;
            actor.issue_order(UnitOrder::attack(target), false)?;
            actor.weapon_phase = WeaponPhase::Windup {
                target_id: target,
                remaining_ticks: 1,
            };
        } else {
            f.world.projectiles.push(sim_core::ActiveProjectile {
                id: 1,
                owner: PlayerId(0),
                attacker_id: Some(shooter),
                source_pos: point(4, 4),
                current_pos: point(9, 4),
                target_id: (route == 1).then_some(target),
                target_pos: point(9, 4),
                target_mover: MoverType::Ground,
                speed: SimScalar::ONE,
                weapon_spec: weapon,
                source_slug: Some(f.world.catalog.get(f.kind)?.slug.clone()),
                remaining_lifetime_ticks: 10,
                strikes: 1,
            });
        }
        f.world.try_load_unit_into_transport(passenger, carrier)?;
        f.world.update_vision();
        let before = f.world.entities[&passenger].clone();
        f.world.step_weapons_and_projectiles();
        assert_eq!(f.world.entities[&passenger], before, "splash route {route}");
        assert!(
            f.world.entities[&target].hp < SimScalar::from_num(100),
            "route must actually deliver damage {route}"
        );
        assert_eq!(f.world.projectiles, []);
    }
    Ok(())
}

#[test]
fn committed_projectile_from_loaded_attacker_still_damages_without_cargo_retaliation() -> TestResult
{
    let mut f = Fixture::new()?;
    f.world.catalog.get_mut(f.kind)?.sight_radius = SimScalar::from_num(20);
    let attacker = f.spawn(0, 4, 4)?;
    let carrier = f.spawn(0, 5, 5)?;
    let victim = f.spawn(1, 9, 4)?;
    f.world.projectiles.push(sim_core::ActiveProjectile {
        id: 1,
        owner: PlayerId(0),
        attacker_id: Some(attacker),
        source_pos: point(4, 4),
        current_pos: point(9, 4),
        target_id: Some(victim),
        target_pos: point(9, 4),
        target_mover: MoverType::Ground,
        speed: SimScalar::ONE,
        weapon_spec: f.weapon.clone(),
        source_slug: Some(f.world.catalog.get(f.kind)?.slug.clone()),
        remaining_lifetime_ticks: 10,
        strikes: 1,
    });
    f.world.try_load_unit_into_transport(attacker, carrier)?;
    let before = f.world.entities[&attacker].clone();
    f.world.update_vision();
    f.world.step_weapons_and_projectiles();
    assert!(f.world.entities[&victim].hp < SimScalar::from_num(100));
    assert_eq!(f.world.entities[&victim].current_target, None);
    assert!(
        !matches!(f.world.entities[&victim].order_queue.front(), Some(UnitOrder::AttackTarget { target, .. }) if *target == attacker)
    );
    assert_eq!(f.world.entities[&attacker], before);
    f.world.apply_combat_retaliation(attacker, victim);
    assert_eq!(f.world.entities[&attacker], before);
    Ok(())
}

#[test]
fn prepared_mutual_lethal_shots_retain_actual_sparse_owners_and_origins() -> TestResult {
    let mut f = Fixture::new()?;
    let mut weapon = f.weapon.clone();
    weapon.base_damage = SimScalar::from_num(1000);
    weapon.attacks_count = 1;
    weapon.max_range = SimScalar::from_num(20);
    weapon.projectile_speed = None;
    weapon.splash = None;
    weapon.visual_kind = sim_types::WeaponVisualKind::Hitscan;
    let arch = f.world.catalog.get_mut(f.kind)?;
    arch.weapon = Some(weapon.clone());
    arch.ground_weapon = Some(weapon);
    let a = f.spawn(2, 8, 8)?;
    let b = f.spawn(7, 10, 8)?;
    for (id, target) in [(a, b), (b, a)] {
        let actor = f.world.entities.get_mut(&id).ok_or("prepared shooter")?;
        actor.issue_order(UnitOrder::attack(target), false)?;
        actor.weapon_phase = WeaponPhase::Windup {
            target_id: target,
            remaining_ticks: 1,
        };
    }
    f.world.step_weapons_and_projectiles();
    assert!(!f.world.entities[&a].is_alive());
    assert!(!f.world.entities[&b].is_alive());
    let events = f.world.build_render_snapshot().combat_events;
    let origins: Vec<_> = events
        .into_iter()
        .filter_map(|event| match event {
            CombatEventVisual::HitscanTracer { from, owner, .. } => {
                Some((owner, [from[0].to_bits(), from[1].to_bits()]))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        origins,
        vec![
            (PlayerId(2), [8.0_f32.to_bits(), 8.0_f32.to_bits()]),
            (PlayerId(7), [10.0_f32.to_bits(), 8.0_f32.to_bits()])
        ]
    );
    Ok(())
}
