//! The cast pipeline of effect-tree abilities (plan `2026-11-10-spellcaster-mechanics` D-6,
//! D-7, §11 WP1d): approach into range measured edge to edge, payment when the effect runs,
//! the after-cast lock, the single caster of a group command, target vision and filter, origin
//! placement, and the order-level admission the client shares with the tick.
//!
//! Every ability is a scratch entry added to a shipped archetype picked by capability, with a
//! scratch spell catalog, so no test names a shipped unit or spell.

#![allow(clippy::expect_used, clippy::panic, clippy::too_many_lines)]

#[path = "support/effect_world.rs"]
mod support;

use sim_core::catalog::{SpellCatalog, UnitArchetype};
use sim_core::SimWorld;
use sim_map::{CellFlags, CliffLevel, GameMap, MapCell};
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EntityId, FilterAlliance, MoverType,
    OriginPlacement, PlayerCommand, PlayerId, ShapeKind, SimScalar, SimVec2, TargetFilter, Tick,
    TickBundle, UnitOrder, UnitTypeId,
};

const STATUSES: &str = r#"(name: "marked", duration_ticks: 500.0)"#;

/// `bolt` hits a unit; `zone` opens a long-lived area at a point; `mark` applies a status.
const EFFECTS: &str = r#"
    (name: "bolt", node: Damage((amount: 10.0))),
    (name: "mark", node: ApplyStatus((status: "marked"))),
    (name: "zone", node: Persistent((anchor: TargetPoint, period_ticks: 1000.0, count: 1,
        periodic: Some(Search((radius: 1.0, child: Damage((amount: 1.0)))))))),
    (name: "pull", node: Persistent((anchor: TargetUnit, period_ticks: 1000.0, count: 1,
        periodic: Some(Displace((flight_ticks: 10)))))),
"#;

/// A scratch ability of `kind`, rooted at `effect`, with range 5 and no cost.
fn ability(kind: AbilityTargetKind, effect: &str) -> AbilitySpec {
    AbilitySpec {
        name: format!("Scratch {effect}"),
        target_kind: kind,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: SimScalar::from_i32(5),
        effect: AbilityEffectSpec::Effect(effect.into()),
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    }
}

fn pick(world: &SimWorld, accept: impl Fn(&UnitArchetype) -> bool) -> UnitTypeId {
    world
        .catalog
        .archetypes()
        .filter(|a| !matches!(a.shape, ShapeKind::BuildingDome { .. }))
        .filter(|a| a.max_speed > SimScalar::ZERO && a.max_hp >= SimScalar::from_i32(40))
        .find(|a| accept(a))
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has such a unit")
}

fn unarmed(world: &SimWorld) -> UnitTypeId {
    pick(world, |a| {
        a.weapon.is_none()
            && a.ground_weapon.is_none()
            && a.air_weapon.is_none()
            && !a.is_detector
            && a.mover_type == MoverType::Ground
    })
}

/// A mobile ground archetype armed with a ranged ground weapon of at least 4.
fn ranged(world: &SimWorld) -> UnitTypeId {
    pick(world, |a| {
        a.mover_type == MoverType::Ground
            && a.ground_weapon
                .as_ref()
                .or(a.weapon.as_ref())
                .is_some_and(|w| w.max_range >= SimScalar::from_i32(4))
    })
}

/// Gives `kind` the ability `spec` and 200 energy without regeneration; returns its index.
fn grant(world: &mut SimWorld, kind: UnitTypeId, spec: AbilitySpec) -> u8 {
    let arch = world.catalog.get_mut(kind).expect("listed");
    arch.max_energy = SimScalar::from_i32(200);
    arch.energy_regen_per_tick = SimScalar::ZERO;
    arch.abilities.push(spec);
    u8::try_from(arch.abilities.len() - 1).expect("few abilities")
}

/// A flat 64 × 64 world with the scratch spell tables.
fn world() -> SimWorld {
    support::world(STATUSES, EFFECTS)
}

/// A world on `map` with the scratch spell tables.
fn world_on(map: GameMap) -> SimWorld {
    let mut world = sim_fixtures::world(map, 11);
    let spells = SpellCatalog::from_ron_strs(
        &format!("(statuses: [{STATUSES}])"),
        &format!("(effects: [{EFFECTS}])"),
    )
    .expect("invariant: valid scratch tables");
    world
        .catalog
        .set_spell_catalog(spells)
        .expect("invariant: the shipped abilities name no effect");
    world
}

fn spawn(world: &mut SimWorld, kind: UnitTypeId, owner: u8, x: i32, y: i32) -> EntityId {
    world
        .spawn_unit(kind, PlayerId(owner), SimVec2::from_i32(x, y))
        .expect("invariant: a free cell")
}

fn energize(world: &mut SimWorld, id: EntityId, energy: i32) {
    world.entities.get_mut(&id).expect("alive").energy = SimScalar::from_i32(energy);
}

fn cast_order(index: u8, at: Option<SimVec2>, on: Option<EntityId>) -> UnitOrder {
    UnitOrder::CastAbility {
        ability_index: index,
        target_pos: at,
        target_entity: on,
    }
}

/// Steps one tick with `commands` from player 0; panics on a diagnostic.
fn tick(world: &mut SimWorld, commands: Vec<(Vec<EntityId>, UnitOrder, bool)>) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: commands
            .into_iter()
            .map(|(entities, order, queue)| PlayerCommand {
                player_id: PlayerId(0),
                entities,
                order,
                queue,
            })
            .collect(),
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn idle(world: &mut SimWorld, ticks: u32) {
    for _ in 0..ticks {
        tick(world, Vec::new());
    }
}

fn position(world: &SimWorld, id: EntityId) -> SimVec2 {
    world.entities.get(&id).expect("alive").position
}

fn radius(world: &SimWorld, id: EntityId) -> SimScalar {
    let kind = world.entities.get(&id).expect("alive").unit_type_id;
    world.catalog.get(kind).expect("listed").collision_radius
}

/// Ticks until a cast of `order` by `caster` shows by `cast_seen`, recording the caster's
/// position at the end of every tick; returns the cast tick and the positions.
fn cast_and_follow(
    world: &mut SimWorld,
    caster: EntityId,
    order: UnitOrder,
    cast_seen: impl Fn(&SimWorld) -> bool,
) -> (u32, Vec<SimVec2>) {
    let mut trail = vec![position(world, caster)];
    tick(world, vec![(vec![caster], order, false)]);
    trail.push(position(world, caster));
    for _ in 0..200 {
        if cast_seen(world) {
            return (world.current_tick, trail);
        }
        tick(world, Vec::new());
        trail.push(position(world, caster));
    }
    panic!("the cast never happened");
}

#[test]
fn a_point_cast_out_of_range_approaches_and_casts_on_the_first_tick_in_edge_range() {
    let mut w = world();
    let kind = unarmed(&w);
    let index = grant(
        &mut w,
        kind,
        AbilitySpec {
            energy_cost: SimScalar::from_i32(50),
            ..ability(AbilityTargetKind::TargetPosition, "zone")
        },
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    energize(&mut w, caster, 100);
    idle(&mut w, 2);
    let start_tick = w.current_tick;
    let point = SimVec2::from_i32(30, 10);
    let reach = SimScalar::from_i32(5) + radius(&w, caster);
    let (cast, trail) =
        cast_and_follow(&mut w, caster, cast_order(index, Some(point), None), |w| {
            !w.effects().persistents().is_empty()
        });
    // Positions at the end of each tick since the command: trail[k] ends tick start + k.
    let k = (cast - start_tick) as usize;
    assert!(
        trail[k - 1].distance(point) <= reach,
        "in edge range when the cast tick began"
    );
    assert!(
        trail[k - 2].distance(point) > reach,
        "not in range a tick earlier, so the cast tick is the first tick in range"
    );
    let created = w.effects().persistents()[0].created_tick;
    assert_eq!(created, Tick(cast));
    assert_eq!(
        w.entities.get(&caster).expect("alive").energy,
        SimScalar::from_i32(50),
        "paid once"
    );
    assert!(
        w.entities
            .get(&caster)
            .expect("alive")
            .order_queue
            .is_empty(),
        "the order is done"
    );
}

#[test]
fn energy_is_paid_on_the_cast_tick_not_while_approaching() {
    let mut w = world();
    let kind = unarmed(&w);
    let index = grant(
        &mut w,
        kind,
        AbilitySpec {
            energy_cost: SimScalar::from_i32(50),
            ..ability(AbilityTargetKind::TargetPosition, "zone")
        },
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    energize(&mut w, caster, 100);
    idle(&mut w, 2);
    tick(
        &mut w,
        vec![(
            vec![caster],
            cast_order(index, Some(SimVec2::from_i32(40, 10)), None),
            false,
        )],
    );
    let energy = |w: &SimWorld| w.entities.get(&caster).expect("alive").energy;
    while w.effects().persistents().is_empty() {
        assert_eq!(energy(&w), SimScalar::from_i32(100), "nothing paid yet");
        idle(&mut w, 1);
    }
    assert_eq!(energy(&w), SimScalar::from_i32(50));
}

#[test]
fn a_unit_cast_approaches_until_the_edges_are_in_range() {
    let mut w = world();
    let kind = unarmed(&w);
    let index = grant(
        &mut w,
        kind,
        ability(AbilityTargetKind::TargetEntity, "bolt"),
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    let target = spawn(&mut w, kind, 1, 30, 10);
    // A spotter gives the caster's owner vision of the target.
    spawn(&mut w, kind, 0, 31, 12);
    idle(&mut w, 2);
    let start_tick = w.current_tick;
    let full = w.entities.get(&target).expect("alive").hp;
    let reach = SimScalar::from_i32(5) + radius(&w, caster) + radius(&w, target);
    let at = position(&w, target);
    let (cast, trail) =
        cast_and_follow(&mut w, caster, cast_order(index, None, Some(target)), |w| {
            w.entities.get(&target).expect("alive").hp < full
        });
    let k = (cast - start_tick) as usize;
    assert!(trail[k - 1].distance(at) <= reach, "edges in range");
    assert!(trail[k - 2].distance(at) > reach, "first tick in range");
}

#[test]
fn the_backswing_locks_movement_attack_and_casts_for_exactly_its_ticks() {
    const LOCK: u16 = 6;
    let mut w = world();
    let kind = ranged(&w);
    let victim_kind = unarmed(&w);
    let index = grant(
        &mut w,
        kind,
        AbilitySpec {
            cast_backswing_ticks: LOCK,
            ..ability(AbilityTargetKind::TargetPosition, "zone")
        },
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    idle(&mut w, 1);
    let point = SimVec2::from_i32(10, 12);
    tick(
        &mut w,
        vec![
            (vec![caster], cast_order(index, Some(point), None), false),
            (
                vec![caster],
                UnitOrder::Move {
                    target: SimVec2::from_i32(10, 30),
                },
                true,
            ),
        ],
    );
    let cast = w.current_tick;
    assert_eq!(
        w.effects().persistents().len(),
        1,
        "cast on the command's tick"
    );
    // An enemy appears in weapon range as the lock begins.
    let victim = spawn(&mut w, victim_kind, 1, 13, 10);
    let full = w.entities.get(&victim).expect("alive").hp;
    let stand = position(&w, caster);
    let locked = |w: &SimWorld| w.entities.get(&caster).expect("alive").is_action_locked();
    // Locked on ticks cast .. cast + LOCK − 1: no walk, no shot, no other cast.
    for _ in 1..LOCK {
        assert!(locked(&w));
        assert!(
            !w.execute_ability(caster, index, Some(point), None),
            "no cast"
        );
        idle(&mut w, 1);
        assert_eq!(
            position(&w, caster),
            stand,
            "tick {}",
            w.current_tick - cast
        );
        assert_eq!(
            w.entities.get(&victim).expect("alive").hp,
            full,
            "no attack"
        );
    }
    assert_eq!(w.current_tick, cast + u32::from(LOCK) - 1);
    assert!(!locked(&w), "free from tick cast + LOCK");
    idle(&mut w, 1);
    assert_ne!(position(&w, caster), stand, "the queued move runs at once");

    // Without a queued order the freed caster attacks.
    let mut w = world();
    let index = grant(
        &mut w,
        kind,
        AbilitySpec {
            cast_backswing_ticks: LOCK,
            ..ability(AbilityTargetKind::TargetPosition, "zone")
        },
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    idle(&mut w, 1);
    tick(
        &mut w,
        vec![(vec![caster], cast_order(index, Some(point), None), false)],
    );
    let victim = spawn(&mut w, victim_kind, 1, 13, 10);
    let full = w.entities.get(&victim).expect("alive").hp;
    idle(&mut w, u32::from(LOCK) - 1);
    assert_eq!(
        w.entities.get(&victim).expect("alive").hp,
        full,
        "no attack while locked"
    );
    idle(&mut w, 40);
    assert!(
        w.entities.get(&victim).expect("alive").hp < full,
        "attacks once free"
    );
}

#[test]
fn a_group_command_casts_once_from_the_nearest_then_most_energetic_then_lowest_id() {
    let setup = || {
        let mut w = world();
        let kind = unarmed(&w);
        let index = grant(
            &mut w,
            kind,
            AbilitySpec {
                energy_cost: SimScalar::from_i32(10),
                cast_range: SimScalar::from_i32(30),
                ..ability(AbilityTargetKind::TargetPosition, "zone")
            },
        );
        let far = spawn(&mut w, kind, 0, 10, 10);
        let near_a = spawn(&mut w, kind, 0, 20, 14);
        let near_b = spawn(&mut w, kind, 0, 20, 6);
        for id in [far, near_a, near_b] {
            energize(&mut w, id, 50);
        }
        idle(&mut w, 1);
        (w, index, [far, near_a, near_b])
    };
    let point = SimVec2::from_i32(26, 10);
    let paid = |w: &SimWorld, ids: &[EntityId]| -> Vec<EntityId> {
        ids.iter()
            .copied()
            .filter(|id| w.entities.get(id).expect("alive").energy < SimScalar::from_i32(50))
            .collect()
    };

    // Two equally near casters with equal energy: the lower id casts.
    let (mut w, index, ids) = setup();
    tick(
        &mut w,
        vec![(ids.to_vec(), cast_order(index, Some(point), None), false)],
    );
    assert_eq!(w.effects().persistents().len(), 1, "one cast for the group");
    assert_eq!(paid(&w, &ids), vec![ids[1].min(ids[2])]);

    // More energy wins a tie of distance.
    let (mut w, index, ids) = setup();
    energize(&mut w, ids[2], 60);
    tick(
        &mut w,
        vec![(ids.to_vec(), cast_order(index, Some(point), None), false)],
    );
    let spent = w.entities.get(&ids[2]).expect("alive").energy;
    assert_eq!(
        spent,
        SimScalar::from_i32(50),
        "the most energetic near caster"
    );
    assert_eq!(paid(&w, &ids), Vec::<EntityId>::new());

    // A queued command goes to every caster.
    let (mut w, index, ids) = setup();
    tick(
        &mut w,
        vec![(ids.to_vec(), cast_order(index, Some(point), None), true)],
    );
    assert_eq!(w.effects().persistents().len(), 3);
    assert_eq!(paid(&w, &ids), ids.to_vec());
}

#[test]
fn a_cloaked_enemy_is_a_valid_target_only_under_detection() {
    let mut w = world();
    let kind = unarmed(&w);
    let detector = pick(&w, |a| {
        a.is_detector && a.weapon.is_none() && a.ground_weapon.is_none() && a.air_weapon.is_none()
    });
    let index = grant(
        &mut w,
        kind,
        ability(AbilityTargetKind::TargetEntity, "bolt"),
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    let hidden = spawn(&mut w, kind, 1, 13, 10);
    w.entities.get_mut(&hidden).expect("alive").is_cloaked = true;
    idle(&mut w, 2);
    assert!(!w.would_accept_cast_order(caster, index, None, Some(hidden)));
    let full = w.entities.get(&hidden).expect("alive").hp;
    tick(
        &mut w,
        vec![(vec![caster], cast_order(index, None, Some(hidden)), false)],
    );
    assert_eq!(w.entities.get(&hidden).expect("alive").hp, full, "refused");
    assert!(w
        .entities
        .get(&caster)
        .expect("alive")
        .order_queue
        .is_empty());
    spawn(&mut w, detector, 0, 11, 12);
    idle(&mut w, 2);
    assert!(w.would_accept_cast_order(caster, index, None, Some(hidden)));
    tick(
        &mut w,
        vec![(vec![caster], cast_order(index, None, Some(hidden)), false)],
    );
    assert!(
        w.entities.get(&hidden).expect("alive").hp < full,
        "detected: hit"
    );
}

#[test]
fn the_target_filter_admits_its_units_only() {
    let mut w = world();
    let kind = unarmed(&w);
    let index = grant(
        &mut w,
        kind,
        AbilitySpec {
            target_filter: Some(TargetFilter {
                alliance: FilterAlliance::Enemy,
                without_status: Some("marked".into()),
                ..TargetFilter::default()
            }),
            ..ability(AbilityTargetKind::TargetEntity, "mark")
        },
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    let own = spawn(&mut w, kind, 0, 12, 10);
    let enemy = spawn(&mut w, kind, 1, 13, 10);
    idle(&mut w, 2);
    assert!(
        !w.would_accept_cast_order(caster, index, None, Some(own)),
        "not an enemy"
    );
    assert!(!w.would_accept_cast_order(caster, index, None, Some(caster)));
    assert!(w.would_accept_cast_order(caster, index, None, Some(enemy)));
    assert!(w.execute_ability(caster, index, None, Some(enemy)));
    assert!(
        !w.would_accept_cast_order(caster, index, None, Some(enemy)),
        "without_status: a marked unit is refused"
    );
    assert!(!w.execute_ability(caster, index, None, Some(enemy)));
}

#[test]
fn origin_placement_without_a_valid_point_refuses_the_cast_without_payment() {
    // An unpathable block of ground 12 cells wide around (30, 30).
    let mut map = GameMap::new(64, 64, MapCell::default_ground(CliffLevel(0))).expect("flat");
    for y in 24..36 {
        for x in 24..36 {
            map.cell_mut(x, y)
                .expect("inside")
                .flags
                .remove(CellFlags::GROUND_PATHABLE);
        }
    }
    let mut w = world_on(map);
    let flier = pick(&w, |a| a.mover_type == MoverType::Air);
    let walker = unarmed(&w);
    let index = grant(
        &mut w,
        flier,
        AbilitySpec {
            energy_cost: SimScalar::from_i32(20),
            cast_range: SimScalar::from_i32(9),
            origin: Some(OriginPlacement {
                forward_offset: SimScalar::from_raw(SimScalar::ONE.raw() / 10),
                search_range: SimScalar::from_i32(2),
            }),
            ..ability(AbilityTargetKind::TargetEntity, "pull")
        },
    );
    let over_void = spawn(&mut w, flier, 0, 30, 30);
    let over_ground = spawn(&mut w, flier, 0, 10, 10);
    let target = spawn(&mut w, walker, 0, 30, 21);
    let near = spawn(&mut w, walker, 0, 10, 14);
    for id in [over_void, over_ground] {
        energize(&mut w, id, 100);
    }
    idle(&mut w, 1);
    assert!(
        w.would_accept_cast_order(over_void, index, None, Some(target)),
        "the order is accepted: placement is a cast-time check"
    );
    assert!(!w.would_admit_cast(over_void, index, None, Some(target)));
    assert!(!w.execute_ability(over_void, index, None, Some(target)));
    assert_eq!(
        w.entities.get(&over_void).expect("alive").energy,
        SimScalar::from_i32(100),
        "no payment"
    );
    assert!(w.execute_ability(over_ground, index, None, Some(near)));
    assert_eq!(
        w.entities.get(&over_ground).expect("alive").energy,
        SimScalar::from_i32(80)
    );
    // The origin is the start point ahead of the caster: 0.1 along its heading.
    let origin = w.effects().persistents()[0].context.origin;
    assert!(
        origin.distance(position(&w, over_ground))
            <= SimScalar::from_raw(SimScalar::ONE.raw() / 10 + 1)
    );
}

#[test]
fn would_accept_cast_order_agrees_with_the_tick_on_every_refusal() {
    let mut w = world();
    let kind = unarmed(&w);
    let index = grant(
        &mut w,
        kind,
        AbilitySpec {
            energy_cost: SimScalar::from_i32(50),
            cooldown_ticks: 20,
            ..ability(AbilityTargetKind::TargetEntity, "bolt")
        },
    );
    let caster = spawn(&mut w, kind, 0, 10, 10);
    let target = spawn(&mut w, kind, 1, 12, 10);
    idle(&mut w, 2);
    let full = w.entities.get(&target).expect("alive").hp;
    // Each case: a refusal the order-level admission must report as the tick does.
    let hp = |w: &SimWorld| w.entities.get(&target).expect("alive").hp;
    let try_cast = |w: &mut SimWorld, on: Option<EntityId>| -> (bool, bool) {
        let predicted = w.would_accept_cast_order(caster, index, None, on);
        let before = hp(w);
        tick(w, vec![(vec![caster], cast_order(index, None, on), false)]);
        (predicted, hp(w) < before)
    };
    energize(&mut w, caster, 10);
    assert_eq!(try_cast(&mut w, Some(target)), (false, false), "energy");
    energize(&mut w, caster, 100);
    assert_eq!(try_cast(&mut w, None), (false, false), "no target");
    assert_eq!(
        try_cast(&mut w, Some(EntityId(9999))),
        (false, false),
        "gone"
    );
    assert_eq!(try_cast(&mut w, Some(target)), (true, true), "admitted");
    assert_eq!(try_cast(&mut w, Some(target)), (false, false), "cooldown");
    assert!(hp(&w) < full);
    assert!(
        !w.would_accept_cast_order(caster, index + 1, None, Some(target)),
        "no ability"
    );
}
