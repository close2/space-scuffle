//! # Integration Test: Stutter-Step Kiting & Backswing Cancellation
//!
//! Verifies that issuing a Move command during `WeaponPhase::Backswing`
//! immediately aborts the follow-through animation, shifts the weapon into `Cooldown`,
//! and allows the unit to translate away from a melee chaser while maintaining full DPS.
//!
//! The wind-up before the release is the other half of the swing: it holds its unit unless the
//! weapon fires while moving (`AllowedMovement::Moving`, `StarCraft` II's `AllowedMovement`), whatever
//! the mount (plan `2026-11-23-turret-wind-up` D-6, P-10), under both movement pipelines.
//!
//! A wind-up whose target is lost holds nothing (plan `2026-11-25-wind-up-and-hold-facing`
//! D-1, D-2, B-207): on the tick after its target was killed the unit moves as if it had never
//! started the swing (A-1), and starts its next swing at once when its cooldown allows (A-2).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::uninlined_format_args,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::panic
)]

use sim_core::catalog::{ContactRules, MovementPipeline, UnitArchetype};
use sim_core::{SimWorld, WeaponPhase, WeaponSpec};
use sim_map::{CliffLevel, GameMap, MapCell, ResourceNode};
use sim_types::{
    AllowedMovement, EntityId, MoverType, PlayerCommand, PlayerId, ResourceKind, ResourceNodeId,
    SimAngle, SimScalar, SimVec2, Tick, TickBundle, UnitOrder, UnitTypeId,
};

#[test]
fn test_stutter_step_backswing_cancellation() {
    let default_cell = MapCell::default_ground(CliffLevel(0));
    let map = GameMap::new(64, 64, default_cell).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x1234_5678);

    let player_blue = PlayerId(0);
    let player_red = PlayerId(1);

    // Place Marine at (10.0, 10.0) and Zealot at (14.0, 10.0)
    let marine_pos = SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10));
    let zealot_pos = SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(10));

    let pulse_bot_id = world
        .catalog
        .id_by_slug("pulse_bot")
        .expect("pulse_bot in catalog");
    let psi_grey_id = world
        .catalog
        .id_by_slug("psi_grey")
        .expect("psi_grey in catalog");

    let marine_id = world
        .spawn_unit(pulse_bot_id, player_blue, marine_pos)
        .expect("invariant: spawn Marine");
    let zealot_id = world
        .spawn_unit(psi_grey_id, player_red, zealot_pos)
        .expect("invariant: spawn Zealot");

    // Marine orders AttackTarget(Zealot)
    let attack_cmd = PlayerCommand {
        player_id: player_blue,
        entities: vec![marine_id],
        order: UnitOrder::attack(zealot_id),
        queue: false,
    };
    let bundle_0 = TickBundle {
        tick: Tick(0),
        commands: vec![attack_cmd],
        state_checksum: None,
    };

    // The marine's own swing, read from the catalog so a rebalance of the marine's timings
    // cannot break this test: it checks the cancellation rule, not the numbers.
    let marine_weapon = world
        .catalog
        .get(pulse_bot_id)
        .expect("pulse_bot archetype")
        .weapon
        .clone()
        .expect("pulse_bot has a weapon");
    let damage_point = marine_weapon.damage_point_ticks;
    let backswing = marine_weapon.backswing_ticks;
    assert!(
        damage_point >= 1 && backswing >= 2,
        "the test needs a wind-up and a backswing that outlasts one tick, got {damage_point} and {backswing}"
    );

    // First tick: the command is processed, the target is in range: the wind-up starts.
    world.step(&bundle_0);
    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert!(
            matches!(
                marine.weapon_phase,
                WeaponPhase::Windup { remaining_ticks, .. } if remaining_ticks == damage_point
            ),
            "Expected Windup with {} ticks, got {:?}",
            damage_point,
            marine.weapon_phase
        );
    }

    let empty_bundle = |tick: u32| TickBundle {
        tick: Tick(tick),
        commands: Vec::new(),
        state_checksum: None,
    };

    // The wind-up counts down to its last tick.
    let mut tick = 1_u32;
    for remaining in (1..damage_point).rev() {
        world.step(&empty_bundle(tick));
        tick += 1;
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert!(
            matches!(
                marine.weapon_phase,
                WeaponPhase::Windup { remaining_ticks, .. } if remaining_ticks == remaining
            ),
            "Expected Windup with {} ticks, got {:?}",
            remaining,
            marine.weapon_phase
        );
    }

    // The wind-up finishes: the shot fires, the zealot is damaged, the backswing starts.
    world.step(&empty_bundle(tick));
    tick += 1;
    {
        let zealot = world.entities.get(&zealot_id).expect("zealot exists");
        assert_eq!(
            zealot.shields,
            SimScalar::from_num(44), // 50 - 6 = 44
            "Zealot shields should take 6 damage"
        );
        let marine = world.entities.get(&marine_id).expect("marine exists");
        assert!(
            matches!(
                marine.weapon_phase,
                WeaponPhase::Backswing { remaining_ticks, .. } if remaining_ticks == backswing
            ),
            "Marine should be in its full Backswing immediately after firing, got {:?}",
            marine.weapon_phase
        );
    }

    // Next tick: Move west to the exact legal full-radius map boundary.
    // An x=0 target would correctly reject rather than interrupt the weapon.
    let legal_boundary = world
        .catalog
        .get(pulse_bot_id)
        .expect("selected archetype exists")
        .collision_radius;
    let move_cmd = PlayerCommand {
        player_id: player_blue,
        entities: vec![marine_id],
        order: UnitOrder::Move {
            target: SimVec2::new(legal_boundary, SimScalar::from_num(10)),
        },
        queue: false,
    };
    let bundle_move = TickBundle {
        tick: Tick(tick),
        commands: vec![move_cmd],
        state_checksum: None,
    };

    world.step(&bundle_move);
    assert_eq!(world.movement_outcomes().len(), 1);
    assert!(
        world.movement_outcomes()[0].result.is_ok(),
        "{:?}",
        world.movement_outcomes()
    );
    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        let speed = world
            .catalog
            .get(marine.unit_type_id)
            .expect("pulse_bot archetype")
            .max_speed;
        // Backswing must be cancelled, weapon in Cooldown, and Marine moved west by speed
        assert!(
            matches!(marine.weapon_phase, WeaponPhase::Cooldown),
            "Marine weapon phase should transition to Cooldown on move cancellation, got {:?}",
            marine.weapon_phase
        );
        let expected_x = SimScalar::from_num(10) - speed;
        assert_eq!(
            marine.position.x, expected_x,
            "Marine should translate west"
        );
    }

    // Advance 2 more ticks
    world.step(&empty_bundle(tick + 1));
    world.step(&empty_bundle(tick + 2));

    {
        let marine = world.entities.get(&marine_id).expect("marine exists");
        let speed = world
            .catalog
            .get(marine.unit_type_id)
            .expect("pulse_bot archetype")
            .max_speed;
        let expected_x = SimScalar::from_num(10) - speed * 3;
        assert_eq!(
            marine.position.x, expected_x,
            "Marine should move 3 ticks of speed west"
        );
    }
}

/// A move during a backswing longer than the weapon's cooldown still cancels the backswing at
/// once, and the cooldown keeps running from where it was: the backswing is a lock on orders,
/// and an order lifts it (Q0038, plan `2026-11-04-combat-sc2-alignment` D-2).
///
/// The marine's weapon is replaced by the scratch weapon of `weapon_cadence_tests.rs`
/// (period 10, damage point 1, backswing 13), so the test does not depend on the marine's
/// authored timings.
#[test]
fn a_move_during_a_long_backswing_still_cancels_it_and_keeps_the_cooldown() {
    let map =
        GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x1234_5678);
    let pulse_bot_id = world.catalog.id_by_slug("pulse_bot").expect("pulse_bot");
    let psi_grey_id = world.catalog.id_by_slug("psi_grey").expect("psi_grey");
    {
        let marine = world.catalog.get_mut(pulse_bot_id).expect("pulse_bot");
        for weapon in [
            &mut marine.weapon,
            &mut marine.ground_weapon,
            &mut marine.air_weapon,
        ]
        .into_iter()
        .flatten()
        {
            weapon.period_ticks = 10;
            weapon.damage_point_ticks = 1;
            weapon.backswing_ticks = 13;
        }
    }
    let marine_id = world
        .spawn_unit(
            pulse_bot_id,
            PlayerId(0),
            SimVec2::new(SimScalar::from_num(10), SimScalar::from_num(10)),
        )
        .expect("invariant: spawn marine");
    let zealot_id = world
        .spawn_unit(
            psi_grey_id,
            PlayerId(1),
            SimVec2::new(SimScalar::from_num(14), SimScalar::from_num(10)),
        )
        .expect("invariant: spawn zealot");
    let bundle = |tick: u32, commands: Vec<PlayerCommand>| TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    };

    // Tick 0 starts the one-tick wind-up; tick 1 releases and charges a 9-tick cooldown.
    world.step(&bundle(
        0,
        vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![marine_id],
            order: UnitOrder::attack(zealot_id),
            queue: false,
        }],
    ));
    world.step(&bundle(1, Vec::new()));
    // Two more ticks into the 13-tick backswing.
    world.step(&bundle(2, Vec::new()));
    world.step(&bundle(3, Vec::new()));
    let (cooldown_before, x_before) = {
        let marine = &world.entities[&marine_id];
        assert!(
            matches!(
                marine.weapon_phase,
                WeaponPhase::Backswing {
                    remaining_ticks: 11,
                    ..
                }
            ),
            "the marine should be two ticks into its backswing, got {:?}",
            marine.weapon_phase
        );
        assert_eq!(marine.weapon_cooldown_remaining, 7);
        (marine.weapon_cooldown_remaining, marine.position.x)
    };

    world.step(&bundle(
        4,
        vec![PlayerCommand {
            player_id: PlayerId(0),
            entities: vec![marine_id],
            order: UnitOrder::Move {
                target: SimVec2::new(SimScalar::from_num(2), SimScalar::from_num(10)),
            },
            queue: false,
        }],
    ));
    let marine = &world.entities[&marine_id];
    assert_eq!(
        marine.weapon_phase,
        WeaponPhase::Cooldown,
        "the move must cancel the backswing"
    );
    assert_eq!(
        marine.weapon_cooldown_remaining,
        cooldown_before - 1,
        "the cooldown keeps running through the cancellation"
    );
    assert!(
        marine.position.x < x_before,
        "the marine walks away at once"
    );
}

/// The weapon slots of `archetype` that hold a weapon, mutably.
fn weapons_mut(archetype: &mut UnitArchetype) -> impl Iterator<Item = &mut WeaponSpec> {
    [
        &mut archetype.weapon,
        &mut archetype.ground_weapon,
        &mut archetype.air_weapon,
    ]
    .into_iter()
    .flatten()
}

/// The shipped ground archetype of lowest type id that moves and has a ranged weapon against
/// its own kind: the scratch weapon's carrier, found by property.
fn ranged_ground_unit(world: &SimWorld) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .filter(|archetype| {
            archetype.mover_type == MoverType::Ground
                && archetype.max_speed > SimScalar::ZERO
                && archetype
                    .weapon_against(MoverType::Ground, None)
                    .is_some_and(|weapon| {
                        !weapon.melee && weapon.target_flags.intersects(archetype.attributes)
                    })
        })
        .min_by_key(|archetype| archetype.unit_type_id)
        .expect("the shipped catalog has a moving ground unit with a ranged weapon")
        .unit_type_id
}

/// One tick of a chase that begins in a wind-up, with the carrier's weapons rewritten as a
/// scratch weapon: a 6-tick damage point, the mount `has_turret` (a turret turns at 90° a
/// tick) and `allowed_movement`, under the movement `pipeline`.
///
/// The unit attacks an enemy of its kind 30 world units east, which holds; after two ticks of
/// walking its weapon is put into a wind-up against that enemy, far out of reach, so the chase
/// would move it, or, when `target_gone`, against an entity that no longer exists (a target
/// killed and reaped on the tick before). Returns the unit's displacement in the next movement
/// phase, world units.
fn displacement_in_wind_up(
    pipeline: MovementPipeline,
    has_turret: bool,
    allowed_movement: AllowedMovement,
    target_gone: bool,
) -> SimScalar {
    let map =
        GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x1234_5678);
    let rules = world.catalog.movement_rules();
    world.catalog.set_movement_rules(
        rules
            .with_contact(ContactRules {
                pipeline,
                ..rules.contact()
            })
            .expect("invariant: the shipped contact rules are valid"),
    );
    let unit_type = ranged_ground_unit(&world);
    for weapon in weapons_mut(world.catalog.get_mut(unit_type).expect("the carrier")) {
        weapon.has_turret = has_turret;
        weapon.turret_turn_rate = SimAngle::DEG_90;
        weapon.allowed_movement = allowed_movement;
        weapon.damage_point_ticks = 6;
        weapon.period_ticks = weapon.period_ticks.max(12);
    }
    let at = |x: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(20));
    let unit = world
        .spawn_unit(unit_type, PlayerId(0), at(10))
        .expect("invariant: spawn the unit");
    let enemy = world
        .spawn_unit(unit_type, PlayerId(1), at(40))
        .expect("invariant: spawn the enemy");
    let command = |player: u8, entity: EntityId, order: UnitOrder| PlayerCommand {
        player_id: PlayerId(player),
        entities: vec![entity],
        order,
        queue: false,
    };
    let bundle = |tick: u32, commands: Vec<PlayerCommand>| TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    };
    world.step(&bundle(
        0,
        vec![
            command(0, unit, UnitOrder::attack(enemy)),
            command(1, enemy, UnitOrder::HoldPosition),
        ],
    ));
    world.step(&bundle(1, Vec::new()));
    let before = {
        let entity = world.entities.get_mut(&unit).expect("the unit");
        assert_ne!(entity.position, at(10), "premise: the chase walks the unit");
        entity.weapon_phase = WeaponPhase::Windup {
            target_id: if target_gone {
                EntityId(u32::MAX)
            } else {
                enemy
            },
            remaining_ticks: 6,
        };
        entity.position
    };
    // The movement phase alone: the property is its own (the combat phase, which runs after it,
    // ends this swing on an enemy out of sight).
    world.step_movement_and_steering();
    assert!(
        world.drain_diagnostics().next().is_none(),
        "the movement phase is healthy"
    );
    world.entities[&unit].position.distance(before)
}

/// P-10 (plan `2026-11-23-turret-wind-up` D-6, B-90, B-45): a unit in a wind-up whose weapon
/// is not `Moving` stands that tick, turreted or not; a unit whose weapon is `Moving` keeps
/// walking through its wind-up, turreted or not. The mount plays no part, and both movement
/// pipelines hold alike (Amendment 2).
#[test]
fn a_wind_up_holds_its_unit_unless_its_weapon_fires_while_moving() {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        for has_turret in [false, true] {
            for allowed in [
                AllowedMovement::Stopped,
                AllowedMovement::Slowing,
                AllowedMovement::Moving,
            ] {
                let walked = displacement_in_wind_up(pipeline, has_turret, allowed, false);
                if allowed == AllowedMovement::Moving {
                    assert!(
                        walked > SimScalar::ZERO,
                        "{pipeline:?}, turret {has_turret}: a `Moving` weapon's wind-up does not hold its unit"
                    );
                } else {
                    assert_eq!(
                        walked,
                        SimScalar::ZERO,
                        "{pipeline:?}, turret {has_turret}, {allowed:?}: the wind-up holds the unit"
                    );
                }
            }
        }
    }
}

/// D-2 of plan `2026-11-25-wind-up-and-hold-facing` (B-207; restates the pin of plan
/// `2026-11-23-turret-wind-up` Amendment 2): a wind-up whose target is gone (killed and reaped on
/// the tick before) holds nothing, whatever its weapon's `AllowedMovement` and mount, under both
/// movement pipelines. `StarCraft` II cancels an attack whose target dies before the damage
/// point. In a full tick the combat-side pass `SimWorld::end_lost_swings` has already ended such
/// a swing before movement; this is the movement phase called alone.
#[test]
fn a_wind_up_whose_target_is_gone_holds_nothing() {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        for has_turret in [false, true] {
            for allowed in [
                AllowedMovement::Stopped,
                AllowedMovement::Slowing,
                AllowedMovement::Moving,
            ] {
                assert!(
                    displacement_in_wind_up(pipeline, has_turret, allowed, true) > SimScalar::ZERO,
                    "{pipeline:?}, turret {has_turret}, {allowed:?}: a swing whose target is gone holds nothing"
                );
            }
        }
    }
}

/// The walking order of the unit of [`walk_after_a_kill`].
#[derive(Clone, Copy, Debug)]
enum Walking {
    /// An attack-move to a point far east.
    AttackMove,
    /// An attack on the victim, with a move far east queued behind it.
    MoveQueuedBehindAttack,
    /// A harvester's walk to a mineral field far east: its task goal.
    TaskGoal,
}

/// The mineral field of [`walk_after_a_kill`]'s map, a cell.
const FIELD: (u32, u32) = (50, 20);

/// A flat 64 × 64 map with one mineral field at [`FIELD`].
fn map_with_a_field() -> GameMap {
    let centre = SimVec2::new(
        SimScalar::from_num(FIELD.0) + SimScalar::HALF,
        SimScalar::from_num(FIELD.1) + SimScalar::HALF,
    );
    GameMap::from_cells(
        64,
        64,
        vec![MapCell::default_ground(CliffLevel(0)); 64 * 64],
        vec![ResourceNode {
            pos: centre,
            kind: ResourceKind::Mineral,
            remaining: 20_000,
            cell: FIELD,
            placed: None,
        }],
        Vec::new(),
    )
    .expect("invariant: a flat map with one field is valid")
}

/// A-1 of plan `2026-11-25-wind-up-and-hold-facing`: the position, after tick T + 1, of a unit
/// whose wind-up target was killed and reaped on tick T, under the movement `pipeline`.
///
/// The unit (the scratch carrier of [`ranged_ground_unit`], or a harvester for
/// [`Walking::TaskGoal`], with its weapons rewritten as in [`displacement_in_wind_up`]) walks
/// two ticks under `walking`; then an enemy, the victim, is killed and reaped (life 0, then the
/// reap phase), and the unit is put into a wind-up against it when `in_wind_up`, or left as it
/// is otherwise. Returns the unit's position after one more full tick and before it.
fn walk_after_a_kill(
    pipeline: MovementPipeline,
    has_turret: bool,
    allowed_movement: AllowedMovement,
    walking: Walking,
    in_wind_up: bool,
) -> (SimVec2, SimVec2) {
    let mut world = sim_fixtures::world(map_with_a_field(), 0x1234_5678);
    let rules = world.catalog.movement_rules();
    world.catalog.set_movement_rules(
        rules
            .with_contact(ContactRules {
                pipeline,
                ..rules.contact()
            })
            .expect("invariant: the shipped contact rules are valid"),
    );
    let enemy_type = ranged_ground_unit(&world);
    let unit_type = match walking {
        Walking::TaskGoal => {
            world
                .catalog
                .archetypes()
                .filter(|archetype| archetype.capabilities.harvester.is_some())
                .min_by_key(|archetype| archetype.unit_type_id)
                .expect("the shipped catalog has a harvester")
                .unit_type_id
        }
        Walking::AttackMove | Walking::MoveQueuedBehindAttack => enemy_type,
    };
    for weapon in weapons_mut(
        world
            .catalog
            .get_mut(unit_type)
            .expect("the unit's archetype"),
    ) {
        weapon.has_turret = has_turret;
        weapon.turret_turn_rate = SimAngle::DEG_90;
        weapon.allowed_movement = allowed_movement;
        weapon.damage_point_ticks = 6;
        weapon.period_ticks = weapon.period_ticks.max(12);
    }
    let at = |x: i32, y: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(y));
    let unit = world
        .spawn_unit(unit_type, PlayerId(0), at(10, 20))
        .expect("invariant: spawn the unit");
    // An enemy far out of every reach keeps the match running once the victim is reaped.
    let anchor = world
        .spawn_unit(enemy_type, PlayerId(1), at(60, 60))
        .expect("invariant: spawn the anchor");
    let victim = world
        .spawn_unit(enemy_type, PlayerId(1), at(30, 45))
        .expect("invariant: spawn the victim");
    let command = |player: u8, entity: EntityId, order: UnitOrder, queue: bool| PlayerCommand {
        player_id: PlayerId(player),
        entities: vec![entity],
        order,
        queue,
    };
    let far_east = at(56, 20);
    let mut orders = vec![
        command(1, anchor, UnitOrder::HoldPosition, false),
        command(1, victim, UnitOrder::HoldPosition, false),
    ];
    match walking {
        Walking::AttackMove => {
            orders.push(command(
                0,
                unit,
                UnitOrder::AttackMove { target: far_east },
                false,
            ));
        }
        Walking::MoveQueuedBehindAttack => {
            orders.push(command(0, unit, UnitOrder::attack(victim), false));
            orders.push(command(0, unit, UnitOrder::Move { target: far_east }, true));
        }
        Walking::TaskGoal => orders.push(command(
            0,
            unit,
            UnitOrder::Harvest {
                target: ResourceNodeId(0),
            },
            false,
        )),
    }
    let bundle = |tick: u32, commands: Vec<PlayerCommand>| TickBundle {
        tick: Tick(tick),
        commands,
        state_checksum: None,
    };
    world.step(&bundle(0, orders));
    world.step(&bundle(1, Vec::new()));
    assert!(
        world.drain_diagnostics().next().is_none(),
        "the walk is healthy"
    );
    // Tick T's combat kills the victim and its reap phase removes it.
    world
        .entities
        .get_mut(&victim)
        .expect("the victim lives until now")
        .hp = SimScalar::ZERO;
    world.reap_dead_and_emit_shatter();
    assert!(
        !world.entities.contains_key(&victim),
        "the victim is reaped"
    );
    let before = {
        let entity = world.entities.get_mut(&unit).expect("the unit");
        assert_ne!(
            entity.position,
            at(10, 20),
            "premise: the order walks the unit"
        );
        if in_wind_up {
            entity.weapon_phase = WeaponPhase::Windup {
                target_id: victim,
                remaining_ticks: 6,
            };
        }
        entity.position
    };
    world.step(&bundle(2, Vec::new()));
    assert!(
        world.drain_diagnostics().next().is_none(),
        "tick T + 1 is healthy"
    );
    (world.entities[&unit].position, before)
}

/// A-1 of plan `2026-11-25-wind-up-and-hold-facing` (D-1, B-207): on the tick after its target
/// was killed and reaped, a unit that was winding up moves exactly as the same unit would if it
/// had never started the swing: the lost swing holds nothing. For an attack-move and a task's
/// walk that is a walk; for an attack order with a move queued behind it, whatever its pursuit
/// of the gone target does that tick. Every `AllowedMovement`, both mounts, both pipelines.
#[test]
fn a_wind_up_whose_target_was_killed_holds_nothing_on_the_next_tick() {
    for pipeline in [MovementPipeline::Solver, MovementPipeline::Legacy] {
        for has_turret in [false, true] {
            for allowed in [
                AllowedMovement::Stopped,
                AllowedMovement::Slowing,
                AllowedMovement::Moving,
            ] {
                for walking in [
                    Walking::AttackMove,
                    Walking::MoveQueuedBehindAttack,
                    Walking::TaskGoal,
                ] {
                    let case =
                        format!("{pipeline:?}, turret {has_turret}, {allowed:?}, {walking:?}");
                    let (after, before) =
                        walk_after_a_kill(pipeline, has_turret, allowed, walking, true);
                    let (control, _) =
                        walk_after_a_kill(pipeline, has_turret, allowed, walking, false);
                    assert_eq!(
                        after, control,
                        "{case}: the lost swing moves the unit as no swing would"
                    );
                    if !matches!(walking, Walking::MoveQueuedBehindAttack) {
                        assert_ne!(after, before, "{case}: the unit walks on");
                    }
                }
            }
        }
    }
}

/// The scene of A-2 (plan `2026-11-25-wind-up-and-hold-facing`): a holding unit with the
/// scratch weapon of [`displacement_in_wind_up`] (hull, `Stopped`, damage point 6, period 12,
/// reach 6, no random delay) and two enemies east of it on its row, 3 and 4 world units away,
/// both in its band and its arc. Steps until the unit winds up, kills and reaps the enemy it
/// winds up against on that tick T, then sets the unit's cooldown to `cooldown` ticks. Returns
/// the world, the unit, the other enemy and T.
fn second_enemy_scene(cooldown: u16) -> (SimWorld, EntityId, EntityId, u32) {
    let map =
        GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("invariant: valid map");
    let mut world = sim_fixtures::world(map, 0x0A2A_2A2A);
    sim_fixtures::without_random_delays(&mut world.catalog);
    let unit_type = ranged_ground_unit(&world);
    for weapon in weapons_mut(world.catalog.get_mut(unit_type).expect("the carrier")) {
        weapon.has_turret = false;
        weapon.allowed_movement = AllowedMovement::Stopped;
        weapon.damage_point_ticks = 6;
        weapon.period_ticks = weapon.period_ticks.max(12);
        weapon.min_range = SimScalar::ZERO;
        weapon.max_range = SimScalar::from_i32(6);
    }
    let at = |x: i32| SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(30));
    let unit = world
        .spawn_unit(unit_type, PlayerId(0), at(20))
        .expect("invariant: spawn the unit");
    let near = world
        .spawn_unit(unit_type, PlayerId(1), at(23))
        .expect("invariant: spawn the near enemy");
    let far = world
        .spawn_unit(unit_type, PlayerId(1), at(24))
        .expect("invariant: spawn the far enemy");
    for id in [unit, near, far] {
        let entity = world.entities.get_mut(&id).expect("invariant: spawned");
        entity.max_hp = SimScalar::from_i32(100_000);
        entity.hp = SimScalar::from_i32(100_000);
    }
    let hold = |player: u8, entity: EntityId| PlayerCommand {
        player_id: PlayerId(player),
        entities: vec![entity],
        order: UnitOrder::HoldPosition,
        queue: false,
    };
    let mut commands = vec![hold(0, unit), hold(1, near), hold(1, far)];
    for tick in 0..40_u32 {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: std::mem::take(&mut commands),
            state_checksum: None,
        });
        if let WeaponPhase::Windup { target_id, .. } = world.entities[&unit].weapon_phase {
            assert_eq!(
                world.entities[&unit].weapon_cooldown_remaining, 0,
                "premise: the first swing"
            );
            let other = if target_id == near { far } else { near };
            world
                .entities
                .get_mut(&target_id)
                .expect("the swing's target")
                .hp = SimScalar::ZERO;
            world.reap_dead_and_emit_shatter();
            world
                .entities
                .get_mut(&unit)
                .expect("the unit")
                .weapon_cooldown_remaining = cooldown;
            let killed_on = world.current_tick;
            return (world, unit, other, killed_on);
        }
    }
    panic!("the holding unit never wound up against an enemy in its band");
}

/// The tick, after `t`, on which `unit` first winds up against `target`, within 30 ticks.
fn next_swing_at(world: &mut SimWorld, unit: EntityId, target: EntityId, t: u32) -> u32 {
    for tick in t + 1..t + 30 {
        world.step(&TickBundle {
            tick: Tick(tick),
            commands: Vec::new(),
            state_checksum: None,
        });
        if matches!(
            world.entities[&unit].weapon_phase,
            WeaponPhase::Windup { target_id, .. } if target_id == target
        ) {
            return world.current_tick;
        }
    }
    panic!("no swing against the second enemy within 30 ticks");
}

/// A-2 of plan `2026-11-25-wind-up-and-hold-facing` (D-1, B-207): with its cooldown at 0 and a
/// second enemy in its band and arc, a unit whose wind-up target was killed on tick T starts its
/// next swing on T + 1; the lost swing costs it no tick.
#[test]
fn the_next_swing_starts_on_the_tick_after_a_kill() {
    let (mut world, unit, other, t) = second_enemy_scene(0);
    assert_eq!(next_swing_at(&mut world, unit, other, t), t + 1);
}

/// A-2 of plan `2026-11-25-wind-up-and-hold-facing`: a unit whose cooldown has `k` ticks left
/// when its wind-up target is killed on tick T starts its next swing on the tick its cooldown
/// ends, T + k, no later than before the plan.
#[test]
fn a_cooling_weapon_starts_its_next_swing_when_the_cooldown_ends() {
    for cooldown in [2_u16, 5] {
        let (mut world, unit, other, t) = second_enemy_scene(cooldown);
        assert_eq!(
            next_swing_at(&mut world, unit, other, t),
            t + u32::from(cooldown),
            "cooldown {cooldown}"
        );
    }
}
