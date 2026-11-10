//! Dynamic checksum sensitivity and replay verification through public APIs.
//!
//! Independent single-field mutations expose omissions before divergent state has
//! produced visible movement or damage. Replay checks compare full entities as well
//! as digests; catalog identities are selected by capabilities, never hardcoded.

#![allow(clippy::expect_used, clippy::too_many_lines, clippy::type_complexity)]

use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sim_core::{
    ActiveBuff, ActiveHazardZone, ActiveProjectile, Entity, SimWorld, WeaponPhase, WeaponSpec,
};
use sim_map::{
    CellFlags, CliffLevel, GameMap, MapCell, PlayerVisionMap, RampDir, ResourceKind, ResourceNode,
    TerrainKind, VisionSource, Watchtower,
};
use sim_types::{
    AnimState, ArmorClassId, AttributeFlags, EntityId, MoverType, PlayerCommand, PlayerId,
    PlayerSlot, ProjectileTargetLossBehavior, SimAngle, SimScalar, SimVec2, SplashSpec, Tick,
    TickBundle, UnitOrder, UnitStatDeltas, WeaponClassId, WeaponVisualKind,
};
use sim_types::{Attribute, AttributeBonuses};
use std::collections::BTreeSet;

/// The first player slot, which every mutation below edits.
const P0: PlayerSlot = PlayerSlot::ALL[0];

fn point(x: i32, y: i32) -> SimVec2 {
    SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y))
}

fn world() -> SimWorld {
    let map = GameMap::from_cells(
        16,
        16,
        vec![MapCell::default_ground(CliffLevel(0)); 256],
        vec![ResourceNode {
            pos: point(12, 12),
            kind: ResourceKind::Mineral,
            remaining: 500,
            cell: (12, 12),
        }],
        vec![Watchtower {
            id: EntityId(100),
            pos: point(8, 8),
            capture_radius: SimScalar::ONE,
            reveal_radius: SimScalar::from_i32(3),
            controlling_player: None,
        }],
    )
    .expect("fixture dimensions match its 256 cells");
    let mut world = sim_fixtures::world(map, 1234);
    let unit_type = world
        .catalog
        .archetypes()
        .find(|archetype| archetype.max_speed > SimScalar::ZERO && archetype.weapon.is_some())
        .expect("catalog contains a mobile armed archetype")
        .unit_type_id;
    world
        .spawn_unit(unit_type, PlayerId(0), point(3, 3))
        .expect("selected archetype exists");
    world
}

fn weapon() -> WeaponSpec {
    world()
        .catalog
        .archetypes()
        .find_map(|archetype| archetype.weapon.clone())
        .expect("catalog contains an armed archetype")
}

fn projectile() -> ActiveProjectile {
    ActiveProjectile {
        id: 1,
        owner: PlayerId(0),
        attacker_id: Some(EntityId(1)),
        source_pos: point(1, 1),
        current_pos: point(2, 2),
        target_id: Some(EntityId(2)),
        target_pos: point(3, 3),
        target_mover: MoverType::Ground,
        speed: SimScalar::ONE,
        weapon_spec: weapon(),
        source_slug: Some("fixture-source".to_owned()),
        remaining_lifetime_ticks: 25,
        strikes: 1,
    }
}

fn hazard() -> ActiveHazardZone {
    ActiveHazardZone {
        id: 1,
        owner: PlayerId(0),
        center: point(4, 4),
        radius: SimScalar::ONE,
        delay_ticks: 2,
        remaining_ticks: 25,
        interval_ticks: 3,
        ticks_until_next_interval: 1,
        damage_per_interval: SimScalar::ONE,
        targets_friendly: false,
    }
}

fn assert_mutations<T: Clone + PartialEq>(
    original: &T,
    mutations: &[(&str, fn(&mut T))],
    mut checksum: impl FnMut(&T) -> u64,
) {
    let baseline = checksum(original);
    for (name, mutate) in mutations {
        let mut changed = original.clone();
        mutate(&mut changed);
        assert!(changed != *original, "mutation must change {name}");
        assert_ne!(baseline, checksum(&changed), "checksum omitted {name}");
    }
}

// Each entry performs a real semantic-state mutation; this is not a second encoder.
macro_rules! mutations {
    ($ty:ty; $($field:ident => $value:expr),+ $(,)?) => {
        &[$({
            let mutate: fn(&mut $ty) = |state| { state.$field = $value; };
            (stringify!($field), mutate)
        }),+]
    };
}

#[test]
fn every_public_entity_field_changes_the_world_checksum() {
    let mut world = world();
    let original = world.entities[&EntityId(1)].clone();
    let unit_type = original.unit_type_id;
    assert_mutations(
        &original,
        mutations! { Entity;
            id => EntityId(88),
            unit_type_id => sim_types::UnitTypeId(u16::MAX),
            owner => PlayerId(1),
            position => point(4, 3),
            pos_at_tick_start => point(3, 4),
            velocity => point(1, 2),
            heading => SimAngle::PI,
            turret_angle => SimAngle::PI,
            hp => SimScalar::from_i32(-1),
            shields => SimScalar::from_i32(-1),
            energy => SimScalar::from_i32(-1),
            ticks_since_last_damage => 7,
            order_queue => [UnitOrder::Stop].into(),
            current_funnel_waypoints => vec![point(5, 5)],
            weapon_phase => WeaponPhase::Cooldown,
            weapon_cooldown_remaining => 7,
            pending_strikes => Some(sim_core::PendingStrikes { target_id: EntityId(2), plane: MoverType::Ground, remaining: 1, ticks_until_next: 5, channeled: false }),
            pending_line => Some(sim_core::PendingLine { points: vec![point(1, 2)], ticks_until_next: 2, plane: MoverType::Ground, angle: SimAngle::PI, primary: EntityId(2), struck: vec![EntityId(3)], view: sim_core::PendingLineView { effect: sim_types::EffectSource::default(), line: None, count: 2 } }),
            active_buffs => vec![ActiveBuff { buff_id: 1, remaining_ticks: 2, speed_multiplier: SimScalar::ONE, attack_rate_multiplier: SimScalar::ONE }],
            production_queue => [sim_core::production::ProductionJob { item: sim_core::production::ProductionItem::Unit(sim_types::UnitTypeId(0)), remaining_ticks: 7, total_ticks: 9, reserved_supply: 3, paid_minerals: 5, paid_gas: 6 }].into(),
            task => sim_core::UnitTask { legs: 2, ..Default::default() },
            cargo => Some(sim_core::Cargo { kind: ResourceKind::Gas, amount: 5 }),
            construction => Some(sim_core::production::construction::Construction {
                remaining_ticks: 3,
                total_ticks: 4,
                hp_per_tick: SimScalar::ONE,
                final_hp_residue: SimScalar::ZERO,
                paid_minerals: 100,
                paid_gas: 0,
                last_worked_tick: None,
            }),
            captured_supply_used => 99,
            captured_supply_provided => 99,
            is_mineral_walking => true,
            is_holding_position => true,
            current_anim => AnimState::Attack,
            last_hit_angle => SimAngle::PI,
            shield_hit_flash_ticks => 7,
            hull_hit_flash_ticks => 7,
            current_target => Some(EntityId(2)),
            is_immobile => true,
            unpushable => true,
            is_sieged => true,
            siege_weapon => Some(weapon()),
            active_mode => Some("fixture-mode".to_owned()),
            channeling_remaining_ticks => 7,
            pending_ability => Some((1, Some(point(3, 4)), Some(EntityId(2)))),
            ability_cooldowns => [1, 2, 3, 4],
            last_storm_damage_tick => Some(Tick(0)),
            is_cloaked => true,
            max_hp => SimScalar::from_i32(-1),
            is_completed => false,
            is_invulnerable => true,
            cast_lock_ticks => 9,
            displacement => Some(sim_core::effects::Displacement {
                node: sim_types::EffectId(1),
                context: sim_types::EffectContext {
                    owner: PlayerId(1),
                    caster: Some(EntityId(2)),
                    ability: sim_types::AbilitySource::default(),
                    origin: point(1, 1),
                    source_point: point(2, 2),
                    target: sim_types::EffectTarget::Unit(EntityId(1)),
                },
                from: point(3, 3),
                to: point(4, 4),
                start_tick: Tick(5),
            }),
        },
        |entity| {
            world.entities.insert(EntityId(1), entity.clone());
            world.compute_checksum()
        },
    );
    assert_ne!(unit_type, sim_types::UnitTypeId(u16::MAX));
}

#[test]
fn every_projectile_and_hazard_field_changes_the_world_checksum() {
    let mut world = world();
    assert_mutations(
        &projectile(),
        mutations! { ActiveProjectile;
            id => 2, owner => PlayerId(1), attacker_id => None,
            source_pos => point(2, 1), current_pos => point(3, 2), target_id => None,
            target_pos => point(4, 3), speed => SimScalar::from_i32(2),
            weapon_spec => { let mut changed = weapon(); changed.base_damage += SimScalar::ONE; changed },
            source_slug => None, remaining_lifetime_ticks => 24, strikes => 2,
        },
        |projectile| {
            world.projectiles = vec![projectile.clone()];
            world.compute_checksum()
        },
    );
    assert_mutations(
        &hazard(),
        mutations! { ActiveHazardZone;
            id => 2, owner => PlayerId(1), center => point(5, 4), radius => SimScalar::from_i32(2),
            delay_ticks => 3, remaining_ticks => 24, interval_ticks => 4,
            ticks_until_next_interval => 2, damage_per_interval => SimScalar::from_i32(2), targets_friendly => true,
        },
        |hazard| {
            world.active_hazards = vec![hazard.clone()];
            world.compute_checksum()
        },
    );
}

#[test]
fn embedded_weapon_fields_and_splash_are_sensitive() {
    let mut world = world();
    let mut original = weapon();
    original.name = "fixture-weapon".to_owned();
    original.visual_kind = WeaponVisualKind::Hitscan;
    original.target_flags = AttributeFlags::LIGHT;
    original.min_range = SimScalar::ZERO;
    original.max_range = SimScalar::ONE;
    original.firing_arc = SimAngle::ZERO;
    original.has_turret = false;
    original.turret_turn_rate = SimAngle::ZERO;
    original.base_damage = SimScalar::ONE;
    original.attribute_bonuses = AttributeBonuses::NONE;
    original.attacks_count = 1;
    original.strike_interval_ticks = 0;
    original.followup_max_distance = None;
    original.channeled = false;
    original.period_ticks = 1;
    original.damage_point_ticks = 0;
    original.backswing_ticks = 0;
    original.splash = None;
    original.projectile_speed = None;
    original.weapon_class = None;
    original.upgrade_bonus_per_level = SimScalar::ZERO;
    original.bonus_upgrade_per_level = SimScalar::ZERO;
    original.target_loss_behavior = ProjectileTargetLossBehavior::Fizzle;
    original.max_projectile_lifetime_ticks = 1;
    original.suicide_on_attack = false;
    original.structure_strike = None;
    let splash = SplashSpec {
        zones: vec![
            sim_types::SplashZone {
                radius: SimScalar::ONE,
                fraction: SimScalar::ONE,
            },
            sim_types::SplashZone {
                radius: SimScalar::from_i32(2),
                fraction: SimScalar::HALF,
            },
            sim_types::SplashZone {
                radius: SimScalar::from_i32(3),
                fraction: SimScalar::from_num(0.25),
            },
        ],
        shape: sim_types::SplashShape::Circle,
        friendly_fire: false,
        offset_by_unit_radius: false,
    };
    let line = sim_types::SplashLine {
        origin: sim_types::SplashLineOrigin::Attacker,
        axis: sim_types::SplashLineAxis::Along,
        offsets: vec![SimScalar::ONE],
        period_ticks: 0,
    };
    assert_mutations(
        &original,
        mutations! { WeaponSpec;
                name => "different-weapon".to_owned(), visual_kind => WeaponVisualKind::Projectile,
                target_flags => AttributeFlags::ARMORED, min_range => SimScalar::ONE,
                max_range => SimScalar::from_i32(2), firing_arc => SimAngle::PI, has_turret => true,
                turret_turn_rate => SimAngle::PI, base_damage => SimScalar::from_i32(2),
                attribute_bonuses => AttributeBonuses::NONE.with(Attribute::Massive, SimScalar::ONE), attacks_count => 2,
                strike_interval_ticks => 1, followup_max_distance => Some(SimScalar::ONE), channeled => true,
                period_ticks => 2, damage_point_ticks => 1, backswing_ticks => 1,
                splash => Some(SplashSpec {
            zones: vec![
                sim_types::SplashZone { radius: SimScalar::ONE, fraction: SimScalar::ONE },
                sim_types::SplashZone { radius: SimScalar::ONE, fraction: SimScalar::HALF },
                sim_types::SplashZone { radius: SimScalar::ONE, fraction: SimScalar::from_num(0.25) },
            ],
            shape: sim_types::SplashShape::Circle,
            friendly_fire: true,
            offset_by_unit_radius: false,
        }),
                projectile_speed => Some(SimScalar::ONE), weapon_class => Some(WeaponClassId(3)),
                upgrade_bonus_per_level => SimScalar::ONE, bonus_upgrade_per_level => SimScalar::ONE,
                target_loss_behavior => ProjectileTargetLossBehavior::ImpactLastKnownPosition,
                max_projectile_lifetime_ticks => 2, suicide_on_attack => true,
                structure_strike => Some(sim_types::StructureStrike {
                    damage: SimScalar::ONE, upgrade_per_level: SimScalar::ONE, ignores_armor: true,
                }),
            },
        |weapon| {
            world
                .entities
                .get_mut(&EntityId(1))
                .expect("fixture entity exists")
                .siege_weapon = Some(weapon.clone());
            world.compute_checksum()
        },
    );
    assert_mutations(
        &splash,
        mutations! { SplashSpec;
            zones => vec![sim_types::SplashZone { radius: SimScalar::ONE, fraction: SimScalar::ONE }],
            shape => sim_types::SplashShape::Line(sim_types::SplashLine {
                origin: sim_types::SplashLineOrigin::Attacker,
                axis: sim_types::SplashLineAxis::Along,
                offsets: vec![SimScalar::ONE],
                period_ticks: 0,
            }),
            friendly_fire => true,
        },
        |splash| {
            let mut weapon = original.clone();
            weapon.splash = Some(splash.clone());
            world
                .entities
                .get_mut(&EntityId(1))
                .expect("fixture entity exists")
                .siege_weapon = Some(weapon);
            world.compute_checksum()
        },
    );
    assert_mutations(
        &line,
        mutations! { sim_types::SplashLine;
            origin => sim_types::SplashLineOrigin::Target,
            axis => sim_types::SplashLineAxis::Across,
            offsets => vec![SimScalar::HALF],
            period_ticks => 2,
        },
        |line| {
            let mut weapon = original.clone();
            let mut changed = splash.clone();
            changed.shape = sim_types::SplashShape::Line(line.clone());
            weapon.splash = Some(changed);
            world
                .entities
                .get_mut(&EntityId(1))
                .expect("fixture entity exists")
                .siege_weapon = Some(weapon);
            world.compute_checksum()
        },
    );
    assert_mutations(
        &splash.zones[0],
        mutations! { sim_types::SplashZone;
            radius => SimScalar::ZERO, fraction => SimScalar::HALF,
        },
        |zone| {
            let mut weapon = original.clone();
            let mut changed = splash.clone();
            changed.zones[0] = *zone;
            weapon.splash = Some(changed);
            world
                .entities
                .get_mut(&EntityId(1))
                .expect("fixture entity exists")
                .siege_weapon = Some(weapon);
            world.compute_checksum()
        },
    );
}

#[test]
fn phase_order_and_target_loss_payloads_remain_distinct() {
    let mut world = world();
    let phases = [
        WeaponPhase::Ready,
        WeaponPhase::Cooldown,
        WeaponPhase::RotatingToTarget {
            target_id: EntityId(1),
        },
        WeaponPhase::RotatingToTarget {
            target_id: EntityId(2),
        },
        WeaponPhase::Windup {
            target_id: EntityId(1),
            remaining_ticks: 1,
        },
        WeaponPhase::Windup {
            target_id: EntityId(2),
            remaining_ticks: 1,
        },
        WeaponPhase::Windup {
            target_id: EntityId(1),
            remaining_ticks: 2,
        },
        WeaponPhase::Backswing {
            target_id: EntityId(1),
            remaining_ticks: 1,
        },
        WeaponPhase::Backswing {
            target_id: EntityId(2),
            remaining_ticks: 1,
        },
        WeaponPhase::Backswing {
            target_id: EntityId(1),
            remaining_ticks: 2,
        },
    ];
    let mut checksums = BTreeSet::new();
    for phase in phases {
        world
            .entities
            .get_mut(&EntityId(1))
            .expect("fixture entity exists")
            .weapon_phase = phase;
        assert!(checksums.insert(world.compute_checksum()));
    }
    let unit_type = world.entities[&EntityId(1)].unit_type_id;
    let orders = [
        UnitOrder::Idle,
        UnitOrder::Move {
            target: point(1, 2),
        },
        UnitOrder::Move {
            target: point(2, 1),
        },
        UnitOrder::AttackMove {
            target: point(1, 2),
        },
        UnitOrder::AttackMove {
            target: point(2, 1),
        },
        UnitOrder::attack(EntityId(1)),
        UnitOrder::attack(EntityId(2)),
        UnitOrder::HoldPosition,
        UnitOrder::Stop,
        UnitOrder::UseStim,
        UnitOrder::ReturnCargo,
        UnitOrder::Produce { unit_type },
        UnitOrder::Produce {
            unit_type: sim_types::UnitTypeId(0),
        },
        UnitOrder::Harvest {
            target: sim_types::ResourceNodeId(1),
        },
        UnitOrder::Harvest {
            target: sim_types::ResourceNodeId(2),
        },
        UnitOrder::Construct {
            foundation: EntityId(1),
        },
        UnitOrder::Construct {
            foundation: EntityId(2),
        },
        UnitOrder::CancelConstruction,
        UnitOrder::Research { upgrade: 1 },
        UnitOrder::Research { upgrade: 2 },
        UnitOrder::CancelProduction { job_index: 0 },
        UnitOrder::CancelProduction { job_index: 1 },
        UnitOrder::Load {
            carrier: EntityId(1),
        },
        UnitOrder::Load {
            carrier: EntityId(2),
        },
        UnitOrder::UnloadAll,
        UnitOrder::Build {
            structure_type: unit_type,
            cell_x: 1,
            cell_y: 2,
        },
        UnitOrder::Build {
            structure_type: unit_type,
            cell_x: 2,
            cell_y: 2,
        },
        UnitOrder::Build {
            structure_type: unit_type,
            cell_x: 1,
            cell_y: 3,
        },
        UnitOrder::Build {
            structure_type: sim_types::UnitTypeId(0),
            cell_x: 1,
            cell_y: 2,
        },
        UnitOrder::CastAbility {
            ability_index: 0,
            target_pos: None,
            target_entity: None,
        },
        UnitOrder::CastAbility {
            ability_index: 1,
            target_pos: None,
            target_entity: None,
        },
        UnitOrder::CastAbility {
            ability_index: 0,
            target_pos: Some(SimVec2::ZERO),
            target_entity: None,
        },
        UnitOrder::CastAbility {
            ability_index: 0,
            target_pos: None,
            target_entity: Some(EntityId(0)),
        },
    ];
    checksums.clear();
    for order in orders {
        world
            .entities
            .get_mut(&EntityId(1))
            .expect("fixture entity exists")
            .order_queue = [order].into();
        assert!(checksums.insert(world.compute_checksum()));
    }
    checksums.clear();
    let mut projectile = projectile();
    for behavior in [
        ProjectileTargetLossBehavior::Fizzle,
        ProjectileTargetLossBehavior::ImpactLastKnownPosition,
        ProjectileTargetLossBehavior::RetargetNearest {
            search_radius: SimScalar::ZERO,
        },
        ProjectileTargetLossBehavior::RetargetNearest {
            search_radius: SimScalar::ONE,
        },
    ] {
        projectile.weapon_spec.target_loss_behavior = behavior;
        world.projectiles = vec![projectile.clone()];
        assert!(checksums.insert(world.compute_checksum()));
    }
}

#[test]
fn player_world_map_and_rng_state_are_observable() {
    let mutations: &[(&str, fn(&mut SimWorld))] = &[
        ("tick", |w| w.current_tick += 1),
        ("seed metadata", |w| w.rng_seed += 1),
        ("RNG key", |w| w.rng = ChaCha8Rng::seed_from_u64(999)),
        ("RNG stream", |w| w.rng.set_stream(1)),
        ("RNG position", |w| {
            w.rng.next_u32();
        }),
        ("entity generator", |w| w.next_entity_id += 1),
        ("projectile generator", |w| w.next_projectile_id += 1),
        ("hazard generator", |w| w.next_hazard_id += 1),
        ("vision pathfinding", |w| w.vision_aware_pathfinding = false),
        ("minerals", |w| w.player_resources[P0].minerals += 1),
        ("gas", |w| w.player_resources[P0].gas += 1),
        ("supply used", |w| w.player_resources[P0].supply_used += 1),
        ("supply max", |w| w.player_resources[P0].supply_max += 1),
        ("uncapped supply", |w| {
            w.player_resources[P0].supply_provided_total += 1;
        }),
        ("weapon level", |w| {
            w.player_upgrades[P0]
                .weapon_levels
                .insert(WeaponClassId(3), 1);
        }),
        ("armor level", |w| {
            w.player_upgrades[P0]
                .armor_levels
                .insert(ArmorClassId(2), 1);
        }),
        ("shield level", |w| w.player_upgrades[P0].shield_level = 1),
        ("completed research", |w| {
            w.player_upgrades[P0]
                .completed_upgrade_slugs
                .insert("fixture-upgrade".to_owned());
        }),
        ("stat modifier", |w| {
            w.player_upgrades[P0]
                .unit_stat_modifiers
                .insert("fixture-unit".to_owned(), UnitStatDeltas::default());
        }),
        ("ability unlock", |w| {
            w.player_upgrades[P0].unlocked_abilities.insert(
                "fixture-unit".to_owned(),
                BTreeSet::from(["fixture-ability".to_owned()]),
            );
        }),
        ("cliff", |w| {
            w.map.cells_mut()[0].cliff_level = CliffLevel(1);
        }),
        ("ramp", |w| w.map.cells_mut()[0].ramp = Some(RampDir::North)),
        ("terrain", |w| {
            w.map.cells_mut()[0].terrain_kind = TerrainKind::Abyss;
        }),
        ("flags", |w| {
            w.map.cells_mut()[0].flags.insert(CellFlags::LOS_BLOCKER);
        }),
        ("occupancy", |w| {
            w.map.cells_mut()[0].occupying_structure = Some(EntityId(1));
        }),
        ("structure", |w| {
            w.map
                .place_structure(EntityId(77), 10, 10, 1, 1)
                .expect("clear fixture footprint");
        }),
        ("resource amount", |w| {
            w.map.resources_mut()[0].remaining -= 1;
        }),
        ("resource type", |w| {
            w.map.resources_mut()[0].kind = ResourceKind::Gas;
        }),
        ("resource position", |w| {
            w.map.resources_mut()[0].pos.x += SimScalar::ONE;
        }),
        ("resource cell", |w| w.map.resources_mut()[0].cell.0 += 1),
        ("watchtower ID", |w| {
            w.map.watchtowers_mut()[0].id = EntityId(101);
        }),
        ("watchtower position", |w| {
            w.map.watchtowers_mut()[0].pos.x += SimScalar::ONE;
        }),
        ("watchtower capture radius", |w| {
            w.map.watchtowers_mut()[0].capture_radius += SimScalar::ONE;
        }),
        ("watchtower reveal radius", |w| {
            w.map.watchtowers_mut()[0].reveal_radius += SimScalar::ONE;
        }),
        ("watchtower ownership", |w| {
            w.map.watchtowers_mut()[0].controlling_player = Some(PlayerId(0));
        }),
        ("fog history", |w| {
            w.vision_maps[P0] = PlayerVisionMap::new(16, 16);
        }),
        ("known obstacle", |w| w.known_blocked_cells[P0][0] = true),
        ("known obstacle length", |w| {
            w.known_blocked_cells[P0].pop();
        }),
        ("navigation revision", |w| {
            w.pathfinder.recompute_clearance(&w.map);
        }),
    ];
    let baseline = world().compute_checksum();
    for (name, mutate) in mutations {
        let mut changed = world();
        mutate(&mut changed);
        assert_ne!(
            baseline,
            changed.compute_checksum(),
            "checksum omitted {name}"
        );
    }
}

#[test]
fn stat_buff_and_pending_ability_payloads_are_sensitive() {
    let mut world = world();
    assert_mutations(
        &UnitStatDeltas::default(),
        mutations! { UnitStatDeltas;
            max_hp_delta => SimScalar::ONE, move_speed_delta => SimScalar::ONE,
            sight_delta => SimScalar::ONE, weapon_range_delta => SimScalar::ONE,
            armor_delta => SimScalar::ONE, attack_cooldown_delta => -1, bonus_damage_delta => SimScalar::ONE,
        },
        |delta| {
            world.player_upgrades[P0]
                .unit_stat_modifiers
                .insert("fixture-unit".to_owned(), *delta);
            world.compute_checksum()
        },
    );
    let buff = ActiveBuff {
        buff_id: 0,
        remaining_ticks: 0,
        speed_multiplier: SimScalar::ONE,
        attack_rate_multiplier: SimScalar::ONE,
    };
    assert_mutations(
        &buff,
        mutations! { ActiveBuff;
            buff_id => 1, remaining_ticks => 1, speed_multiplier => SimScalar::ZERO, attack_rate_multiplier => SimScalar::ZERO,
        },
        |buff| {
            world
                .entities
                .get_mut(&EntityId(1))
                .expect("fixture entity exists")
                .active_buffs = vec![buff.clone()];
            world.compute_checksum()
        },
    );
    let mut checksums = BTreeSet::new();
    for pending in [
        None,
        Some((0, None, None)),
        Some((1, None, None)),
        Some((0, Some(SimVec2::ZERO), None)),
        Some((0, None, Some(EntityId(0)))),
    ] {
        world
            .entities
            .get_mut(&EntityId(1))
            .expect("fixture entity exists")
            .pending_ability = pending;
        assert!(checksums.insert(world.compute_checksum()));
    }
}

#[test]
fn fog_counters_and_stale_clearance_are_detected() {
    let mut world = world();
    let source = VisionSource {
        pos: point(3, 3),
        mover_type: MoverType::Ground,
        sight_radius: SimScalar::ONE,
        is_detector: false,
    };
    world.vision_maps[P0].apply_vision_source(&world.map, &source);
    let once = world.compute_checksum();
    world.vision_maps[P0].apply_vision_source(&world.map, &source);
    assert_ne!(
        once,
        world.compute_checksum(),
        "visible counters must be included independently of visibility state"
    );
    let mut detector_vision = PlayerVisionMap::new(16, 16);
    detector_vision.set_all_fogged();
    detector_vision.apply_vision_source(
        &world.map,
        &VisionSource {
            is_detector: true,
            ..source
        },
    );
    world.vision_maps[P0].begin_tick_update();
    world.vision_maps[P0].apply_vision_source(&world.map, &source);
    let without_detection = world.compute_checksum();
    assert_eq!(world.vision_maps[P0].states(), detector_vision.states());
    assert_eq!(
        world.vision_maps[P0].visible_counter(),
        detector_vision.visible_counter()
    );
    world.vision_maps[P0] = detector_vision;
    assert_ne!(
        without_detection,
        world.compute_checksum(),
        "detector counters must be included independently of visibility counters"
    );

    let mut other = self::world();
    let baseline = other.compute_checksum();
    let mut changed_map = other.map.clone();
    changed_map.cells_mut()[0]
        .flags
        .remove(CellFlags::GROUND_PATHABLE);
    other.pathfinder = sim_map::Pathfinder::new(&changed_map);
    assert_ne!(
        baseline,
        other.compute_checksum(),
        "clearance differs even though world terrain and navigation version are unchanged"
    );
}

#[test]
fn hashing_is_pure_and_ordering_contracts_are_respected() {
    let mut world = world();
    let baseline = world.compute_checksum();
    let mut expected_rng = world.rng.clone();
    for _ in 0..5 {
        assert_eq!(baseline, world.compute_checksum());
    }
    assert_eq!(
        expected_rng.next_u64(),
        world.rng.next_u64(),
        "hashing must not advance RNG"
    );
    let baseline = world.compute_checksum();
    world.last_checksum ^= u64::MAX;
    world.spatial_hash.clear();
    world
        .pending_events
        .push(sim_types::CombatEventVisual::HitscanTracer {
            from: [0.0; 3],
            to: [1.0; 3],
            owner: PlayerId(0),
            source: sim_types::EffectSource::default(),
        });
    assert_eq!(
        baseline,
        world.compute_checksum(),
        "derived output and scratch storage are excluded"
    );

    let entity = world.entities[&EntityId(1)].clone();
    let mut second = entity.clone();
    second.id = EntityId(2);
    world.entities.insert(second.id, second.clone());
    let baseline = world.compute_checksum();
    world.entities.clear();
    world.entities.insert(second.id, second);
    world.entities.insert(entity.id, entity);
    assert_eq!(
        baseline,
        world.compute_checksum(),
        "map insertion order is irrelevant"
    );
    world
        .entities
        .get_mut(&EntityId(1))
        .expect("fixture entity exists")
        .order_queue = [UnitOrder::Stop, UnitOrder::HoldPosition].into();
    let forward = world.compute_checksum();
    world
        .entities
        .get_mut(&EntityId(1))
        .expect("fixture entity exists")
        .order_queue = [UnitOrder::HoldPosition, UnitOrder::Stop].into();
    assert_ne!(
        forward,
        world.compute_checksum(),
        "queue order affects future behavior"
    );
}

#[test]
fn capability_selected_replay_matches_full_state_for_2000_ticks() {
    let mut a = world();
    let mut b = world();
    let unit_type = a.entities[&EntityId(1)].unit_type_id;
    for world in [&mut a, &mut b] {
        world
            .spawn_unit(unit_type, PlayerId(1), point(12, 12))
            .expect("selected archetype exists");
    }
    let mut observed_movement = false;
    for tick in 1..=2000 {
        let mut commands = Vec::new();
        if tick == 1 {
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![EntityId(1)],
                order: UnitOrder::Move {
                    target: point(5, 5),
                },
                queue: false,
            });
            commands.push(PlayerCommand {
                player_id: PlayerId(0),
                entities: vec![EntityId(1)],
                order: UnitOrder::AttackMove {
                    target: point(12, 12),
                },
                queue: true,
            });
        }
        let bundle = TickBundle {
            tick: Tick(tick),
            commands,
            state_checksum: None,
        };
        let left = a.step(&bundle);
        let right = b.step(&bundle);
        assert_eq!(
            left.checksum, right.checksum,
            "replay digest at tick {tick}"
        );
        assert_eq!(a.entities, b.entities, "full entity state at tick {tick}");
        assert_eq!(
            a.projectiles, b.projectiles,
            "full projectile state at tick {tick}"
        );
        assert_eq!(
            a.active_hazards, b.active_hazards,
            "hazard state at tick {tick}"
        );
        assert_eq!(a.player_resources, b.player_resources);
        assert_eq!(a.player_upgrades, b.player_upgrades);
        observed_movement |= a
            .entities
            .get(&EntityId(1))
            .is_some_and(|entity| entity.position != point(3, 3));
    }
    assert!(
        observed_movement,
        "replay must exercise queued movement and attack commands"
    );
}
