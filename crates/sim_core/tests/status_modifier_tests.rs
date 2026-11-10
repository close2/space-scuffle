//! The status modifiers, each read by the system it changes (plan
//! `2026-11-10-spellcaster-mechanics` D-3, §6, §7): stun, movement speed, weapon range with its
//! floor, reveal, suppressed detection, collision, boarding and Blink; and the exclusion of a
//! displaced unit from movement and pushing (D-8).
//!
//! Every status is a scratch catalog entry applied through an `ApplyStatus` node, so the tests
//! exercise the engine's own bookkeeping of the modifiers; no test names a shipped spell.

#![allow(clippy::expect_used)]

#[path = "support/effect_world.rs"]
mod support;

use sim_core::catalog::UnitArchetype;
use sim_core::SimWorld;
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EffectTarget, EntityId, MoverType,
    PlayerCommand, PlayerId, PlayerView, ShapeKind, SimScalar, SimVec2, Tick, TickBundle,
    UnitOrder, UnitTypeId,
};
use support::{context, hp, run, scalar, step};

/// Every scratch status of these tests, and the effects that apply or remove them.
const STATUSES: &str = r#"
    (name: "stun", duration_ticks: 30.0, stun: true),
    (name: "slow", duration_ticks: 200.0, move_speed_factor: 0.25),
    (name: "blind", duration_ticks: 200.0, ranged_weapon_range_delta: -20.0),
    (name: "blind_all", duration_ticks: 200.0, weapon_range_delta: -20.0),
    (name: "reveal", duration_ticks: 200.0, reveal: true),
    (name: "no_detect", duration_ticks: 200.0, suppress_detection: true),
    (name: "ghost", duration_ticks: 200.0, suppress_collision: true),
    (name: "no_board", duration_ticks: 200.0, suppress_boarding: true),
    (name: "no_blink", duration_ticks: 200.0, suppress_blink: true),
"#;

const EFFECTS: &str = r#"
    (name: "stun", node: ApplyStatus((status: "stun"))),
    (name: "slow", node: ApplyStatus((status: "slow"))),
    (name: "blind", node: ApplyStatus((status: "blind"))),
    (name: "blind_all", node: ApplyStatus((status: "blind_all"))),
    (name: "reveal", node: ApplyStatus((status: "reveal"))),
    (name: "no_detect", node: ApplyStatus((status: "no_detect"))),
    (name: "ghost", node: ApplyStatus((status: "ghost"))),
    (name: "no_board", node: ApplyStatus((status: "no_board"))),
    (name: "no_blink", node: ApplyStatus((status: "no_blink"))),
    (name: "unblind", node: RemoveStatus((status: "blind"))),
    (name: "fly", node: Displace((flight_ticks: 10))),
"#;

fn world() -> SimWorld {
    support::world(STATUSES, EFFECTS)
}

/// Applies the scratch effect `name` to `unit`.
fn apply(world: &mut SimWorld, name: &str, unit: EntityId) {
    let ctx = context(world, None, EffectTarget::Unit(unit));
    run(world, name, ctx);
}

/// A mobile, unarmed, non-structure archetype that does not detect.
fn unarmed(world: &SimWorld) -> UnitTypeId {
    pick(world, |a| {
        a.weapon.is_none()
            && a.ground_weapon.is_none()
            && a.air_weapon.is_none()
            && !a.is_detector
            && a.mover_type == MoverType::Ground
    })
}

/// A mobile ground archetype whose ground weapon reaches at least 4 world units.
fn ranged(world: &SimWorld) -> UnitTypeId {
    pick(world, |a| {
        a.mover_type == MoverType::Ground
            && a.ground_weapon
                .as_ref()
                .or(a.weapon.as_ref())
                .is_some_and(|w| w.max_range >= SimScalar::from_i32(4) && !w.melee)
    })
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

fn spawn(world: &mut SimWorld, kind: UnitTypeId, owner: u8, at: SimVec2) -> EntityId {
    world
        .spawn_unit(kind, PlayerId(owner), at)
        .expect("invariant: a free cell")
}

fn position(world: &SimWorld, id: EntityId) -> SimVec2 {
    world.entities.get(&id).expect("alive").position
}

fn order(world: &mut SimWorld, player: u8, units: &[EntityId], order: UnitOrder) {
    let tick = Tick(world.current_tick);
    world.step(&TickBundle {
        tick,
        commands: vec![PlayerCommand {
            player_id: PlayerId(player),
            entities: units.to_vec(),
            order,
            queue: false,
        }],
        state_checksum: None,
    });
    let diagnostics: Vec<_> = world.drain_diagnostics().collect();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

fn steps(world: &mut SimWorld, ticks: u32) {
    for _ in 0..ticks {
        step(world);
    }
}

#[test]
fn a_stunned_unit_neither_walks_nor_attacks_and_its_orders_wait() {
    let mut w = world();
    let kind = ranged(&w);
    let target_kind = unarmed(&w);
    let walker = spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    apply(&mut w, "stun", walker);
    assert!(
        w.entities
            .get(&walker)
            .expect("alive")
            .status_modifiers()
            .stun
    );
    let start = position(&w, walker);
    order(
        &mut w,
        0,
        &[walker],
        UnitOrder::Move {
            target: SimVec2::from_i32(30, 10),
        },
    );
    steps(&mut w, 20);
    assert_eq!(position(&w, walker), start, "stunned: no walk");
    // The status lasts 30 ticks; the move order then runs.
    steps(&mut w, 20);
    assert!(position(&w, walker).x > start.x, "the waiting order runs");

    // A stunned armed unit next to an enemy does not attack it.
    let mut w = world();
    let shooter = spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    let victim = spawn(&mut w, target_kind, 1, SimVec2::from_i32(12, 10));
    apply(&mut w, "stun", shooter);
    let full = hp(&w, victim);
    steps(&mut w, 25);
    assert_eq!(hp(&w, victim), full, "stunned: no attack");
    steps(&mut w, 40);
    assert!(hp(&w, victim) < full, "the unit attacks once the stun ends");
}

#[test]
fn a_speed_factor_slows_the_walk() {
    let mut w = world();
    let kind = unarmed(&w);
    let plain = spawn(&mut w, kind, 0, SimVec2::from_i32(5, 10));
    let slowed = spawn(&mut w, kind, 0, SimVec2::from_i32(5, 20));
    apply(&mut w, "slow", slowed);
    let (a0, b0) = (position(&w, plain), position(&w, slowed));
    order(
        &mut w,
        0,
        &[plain],
        UnitOrder::Move {
            target: SimVec2::from_i32(60, 10),
        },
    );
    order(
        &mut w,
        0,
        &[slowed],
        UnitOrder::Move {
            target: SimVec2::from_i32(60, 20),
        },
    );
    steps(&mut w, 40);
    let normal = position(&w, plain).x - a0.x;
    let quarter = position(&w, slowed).x - b0.x;
    // Acceleration makes the ratio inexact over a short walk; it stays near 1/4.
    let ratio = quarter / normal;
    assert!(
        ratio > scalar("0.2") && ratio < scalar("0.3"),
        "walked {normal:?} and {quarter:?}"
    );
}

#[test]
fn a_ranged_range_reduction_keeps_the_floor_and_spares_melee() {
    let mut w = world();
    let kind = ranged(&w);
    let shooter = spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    let arch = w.catalog.get(kind).expect("listed").clone();
    let weapon = arch
        .ground_weapon
        .clone()
        .or(arch.weapon.clone())
        .expect("armed");
    let entity = w.entities.get(&shooter).expect("alive").clone();
    let full = w.entity_weapon_range(&entity, &weapon, &arch.slug);
    assert_eq!(
        full,
        w.effective_weapon_range(&weapon, &arch.slug, entity.owner)
    );
    apply(&mut w, "blind", shooter);
    let entity = w.entities.get(&shooter).expect("alive").clone();
    let floor = entity.status_modifiers().range_floor;
    let blind = w.catalog.spells().status_id("blind").expect("named");
    let authored = w
        .catalog
        .spells()
        .status(blind)
        .expect("listed")
        .spec
        .range_floor;
    assert_eq!(floor, authored, "the status's floor, by default 0.1 (SU-2)");
    assert!(floor > SimScalar::ZERO && floor < scalar("0.2"));
    assert_eq!(w.entity_weapon_range(&entity, &weapon, &arch.slug), floor);
    let melee = sim_types::WeaponSpec {
        melee: true,
        ..weapon.clone()
    };
    assert_eq!(w.entity_weapon_range(&entity, &melee, &arch.slug), full);
    // An all-weapons reduction reaches a melee weapon too.
    apply(&mut w, "unblind", shooter);
    apply(&mut w, "blind_all", shooter);
    let entity = w.entities.get(&shooter).expect("alive").clone();
    assert_eq!(w.entity_weapon_range(&entity, &melee, &arch.slug), floor);
}

#[test]
fn a_blinded_ranged_unit_cannot_shoot_a_target_beyond_contact() {
    let mut w = world();
    let kind = ranged(&w);
    let target_kind = unarmed(&w);
    let shooter = spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    let victim = spawn(&mut w, target_kind, 1, SimVec2::from_i32(13, 10));
    apply(&mut w, "blind", shooter);
    order(&mut w, 0, &[shooter], UnitOrder::HoldPosition);
    let full = hp(&w, victim);
    steps(&mut w, 40);
    assert_eq!(hp(&w, victim), full, "out of the reduced range");
    apply(&mut w, "unblind", shooter);
    steps(&mut w, 40);
    assert!(hp(&w, victim) < full, "in range again once the status goes");
}

#[test]
fn a_revealed_cloaked_unit_is_attacked_without_detection() {
    let mut w = world();
    let kind = ranged(&w);
    let target_kind = unarmed(&w);
    spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    let hidden = spawn(&mut w, target_kind, 1, SimVec2::from_i32(13, 10));
    w.entities.get_mut(&hidden).expect("alive").is_cloaked = true;
    let full = hp(&w, hidden);
    steps(&mut w, 30);
    assert_eq!(hp(&w, hidden), full, "cloaked and undetected: no attack");
    apply(&mut w, "reveal", hidden);
    assert!(
        !w.entities.get(&hidden).expect("alive").is_concealed(),
        "a revealed unit is not concealed"
    );
    steps(&mut w, 30);
    assert!(hp(&w, hidden) < full, "revealed: attacked");
}

#[test]
fn a_suppressed_detector_reveals_nothing() {
    let mut w = world();
    let plain = unarmed(&w);
    let detector = pick(&w, |a| {
        a.is_detector && a.weapon.is_none() && a.ground_weapon.is_none() && a.air_weapon.is_none()
    });
    let hidden = spawn(&mut w, plain, 1, SimVec2::from_i32(30, 24));
    w.entities.get_mut(&hidden).expect("alive").is_cloaked = true;
    let eye = spawn(&mut w, detector, 0, SimVec2::from_i32(28, 24));
    steps(&mut w, 2);
    let mut view = PlayerView::default();
    w.write_player_view(PlayerId(0), &mut view);
    assert_eq!(view.revealed, [hidden], "the detector reveals");
    apply(&mut w, "no_detect", eye);
    steps(&mut w, 2);
    w.write_player_view(PlayerId(0), &mut view);
    assert_eq!(view.revealed, [], "a suppressed detector does not");
}

#[test]
fn a_unit_without_collision_neither_pushes_nor_is_pushed() {
    let overlap = |ghost: bool| {
        let mut w = world();
        let kind = unarmed(&w);
        let a = spawn(&mut w, kind, 0, SimVec2::from_i32(20, 20));
        let b = spawn(&mut w, kind, 0, SimVec2::from_i32(25, 20));
        w.entities.get_mut(&b).expect("alive").position = scalar_point("20.1", "20");
        if ghost {
            apply(&mut w, "ghost", b);
        }
        let before = (position(&w, a), position(&w, b));
        steps(&mut w, 5);
        before == (position(&w, a), position(&w, b))
    };
    assert!(!overlap(false), "overlapping bodies separate");
    assert!(overlap(true), "a body without collision stays where it is");
}

#[test]
fn a_unit_that_may_not_board_is_refused_by_the_transport() {
    let mut w = world();
    let kind = unarmed(&w);
    let passenger = spawn(&mut w, kind, 0, SimVec2::from_i32(20, 20));
    let carrier = spawn(&mut w, kind, 0, SimVec2::from_i32(22, 20));
    apply(&mut w, "no_board", passenger);
    assert!(matches!(
        w.try_load_unit_into_transport(passenger, carrier),
        Err(sim_core::transport::TransportAdmissionError::Busy(id)) if id == passenger
    ));
    let other = spawn(&mut w, kind, 0, SimVec2::from_i32(24, 20));
    assert!(w.try_load_unit_into_transport(other, carrier).is_ok());
}

#[test]
fn a_unit_that_may_not_blink_is_refused_the_blink() {
    let mut w = world();
    let kind = unarmed(&w);
    let arch = w.catalog.get_mut(kind).expect("listed");
    arch.abilities.push(AbilitySpec {
        name: "Scratch Blink".into(),
        target_kind: AbilityTargetKind::TargetPosition,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 0,
        cast_range: SimScalar::from_i32(8),
        effect: AbilityEffectSpec::BlinkTeleport {
            max_range: SimScalar::from_i32(8),
            cooldown_ticks: 0,
        },
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    });
    let index = u8::try_from(arch.abilities.len() - 1).expect("few abilities");
    let blinker = spawn(&mut w, kind, 0, SimVec2::from_i32(20, 20));
    let other = spawn(&mut w, kind, 0, SimVec2::from_i32(20, 30));
    apply(&mut w, "no_blink", blinker);
    assert!(!w.execute_ability(blinker, index, Some(SimVec2::from_i32(24, 20)), None));
    assert_eq!(position(&w, blinker), SimVec2::from_i32(20, 20));
    assert!(w.execute_ability(other, index, Some(SimVec2::from_i32(24, 30)), None));
}

#[test]
fn a_displaced_unit_flies_straight_through_bodies_and_ignores_its_orders() {
    let mut w = world();
    let kind = unarmed(&w);
    let flown = spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    let bystander = spawn(&mut w, kind, 1, SimVec2::from_i32(15, 10));
    let stand = position(&w, bystander);
    // The context's origin is where the flight lands: 10 world units along x, in 10 ticks.
    let mut ctx = context(&w, None, EffectTarget::Unit(flown));
    ctx.origin = SimVec2::from_i32(20, 10);
    run(&mut w, "fly", ctx);
    order(
        &mut w,
        0,
        &[flown],
        UnitOrder::Move {
            target: SimVec2::from_i32(10, 40),
        },
    );
    for k in 2..=9 {
        step(&mut w);
        // Fixed-point interpolation: within 2^-20 of the straight line's point.
        let gap = position(&w, flown).distance(SimVec2::from_i32(10 + k, 10));
        assert!(
            gap < SimScalar::from_raw(1 << 12),
            "tick {k}: the flight alone moves the unit ({gap:?} off)"
        );
        assert_eq!(position(&w, bystander), stand, "tick {k}: nobody is pushed");
    }
    step(&mut w);
    assert!(
        w.entities
            .get(&flown)
            .expect("alive")
            .displacement
            .is_none(),
        "landed"
    );
}

fn scalar_point(x: &str, y: &str) -> SimVec2 {
    SimVec2::new(scalar(x), scalar(y))
}

#[test]
fn the_cached_modifiers_equal_the_fold_of_the_status_table() {
    use sim_core::effects::StatusModifiers;
    let mut w = world();
    let kind = unarmed(&w);
    let unit = spawn(&mut w, kind, 0, SimVec2::from_i32(10, 10));
    let fold = |w: &SimWorld| StatusModifiers::of(w.effects(), w.catalog.spells(), unit);
    let cached = |w: &SimWorld| w.entities.get(&unit).expect("alive").status_modifiers();
    assert_eq!(cached(&w), StatusModifiers::NEUTRAL);
    for name in ["stun", "slow", "blind", "reveal", "ghost"] {
        apply(&mut w, name, unit);
        assert_eq!(cached(&w), fold(&w), "after {name}");
    }
    assert!(cached(&w).stun && cached(&w).reveal && cached(&w).suppress_collision);
    apply(&mut w, "unblind", unit);
    assert_eq!(cached(&w), fold(&w), "after a removal");
    assert_eq!(cached(&w).ranged_weapon_range_delta, SimScalar::ZERO);
    // The stun expires after 30 ticks, the others after 200.
    steps(&mut w, 31);
    assert!(!cached(&w).stun, "expired");
    assert_eq!(cached(&w), fold(&w), "after an expiry");
    steps(&mut w, 200);
    assert_eq!(cached(&w), StatusModifiers::NEUTRAL);
}
