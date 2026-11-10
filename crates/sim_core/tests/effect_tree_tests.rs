//! The effect-tree engine, node by node, on scratch catalogs (plan
//! `2026-11-10-spellcaster-mechanics` §11, WP1c): each node kind's action, the target
//! contexts of D-15, the evaluation order of D-16 and the status timing of D-4 and D-5.

#![allow(clippy::expect_used, clippy::too_many_lines)]

#[path = "support/effect_world.rs"]
mod support;

use sim_core::effects::PersistentAnchor;
use sim_types::{
    AbilityEffectSpec, AbilitySpec, AbilityTargetKind, EffectTarget, MoverType, PlayerId,
    SimScalar, SimVec2, UnitOrder,
};
use support::{context, effect, hp, run, scalar, step, unit, world};

const NO_STATUSES: &str = "";

#[test]
fn a_set_runs_its_children_in_authored_order() {
    // Damage 10 then halve life: (100 − 10) / 2; the reverse order would give 40.
    let mut w = world(
        NO_STATUSES,
        r#"(name: "set", node: Set((children: [Damage((amount: 10.0)),
            ModifyVitals((life: Some((change_fraction: -0.5))))])))"#,
    );
    let target = unit(&mut w, 0, 10, 10);
    let start = hp(&w, target);
    let ctx = context(&w, None, EffectTarget::Unit(target));
    run(&mut w, "set", ctx);
    let expected = (start - SimScalar::from_i32(10)) * SimScalar::HALF;
    assert_eq!(hp(&w, target), expected);
}

#[test]
fn a_search_admits_its_filter_in_distance_order_up_to_max_count() {
    let mut w = world(
        r#"(name: "mark", duration_ticks: 100.0), (name: "shield", duration_ticks: 100.0)"#,
        r#"(name: "protect", node: ApplyStatus((status: "shield"))),
           (name: "search", node: Search((radius: 3.5, max_count: Some(2),
            filter: (alliance: Friendly, without_status: Some("shield")),
            child: ApplyStatus((status: "mark")))))"#,
    );
    let near = unit(&mut w, 0, 12, 10);
    let middle = unit(&mut w, 0, 10, 13);
    let far = unit(&mut w, 0, 13, 10);
    let shielded = unit(&mut w, 0, 11, 10);
    let enemy = unit(&mut w, 1, 10, 11);
    let outside = unit(&mut w, 0, 14, 10);
    let shield = w.catalog.spells().status_id("shield").expect("named");
    let mark = w.catalog.spells().status_id("mark").expect("named");
    let ctx = context(&w, None, EffectTarget::Unit(shielded));
    run(&mut w, "protect", ctx);
    let centre = context(&w, None, EffectTarget::Point(SimVec2::from_i32(10, 10)));
    run(&mut w, "search", centre);
    let marked: Vec<_> = w
        .effects()
        .statuses()
        .iter()
        .filter(|s| s.status == mark)
        .map(|s| s.entity)
        .collect();
    // Friendly, unshielded, within 3.5: near (2), middle (3), far (3); the two nearest by
    // (distance², id) are near and middle (lower id than far at the same distance).
    assert_eq!(marked, vec![near, middle]);
    assert!(!marked.contains(&far) && !marked.contains(&enemy) && !marked.contains(&outside));
    assert!(w.effects().has_status(shielded, shield));
    let source = w
        .effects()
        .statuses()
        .iter()
        .find(|s| s.status == mark)
        .map(|s| s.context.source_point);
    assert_eq!(
        source,
        Some(SimVec2::from_i32(10, 10)),
        "a search child's source is the centre"
    );
}

#[test]
fn a_persistent_pulses_on_rounded_ticks_and_ends_with_its_unit() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "dot", node: Persistent((anchor: TargetUnit,
            periodic: Some(Damage((amount: 1.0))), period_ticks: 10.196429, count: 7)))"#,
    );
    let target = unit(&mut w, 0, 10, 10);
    let ctx = context(&w, None, EffectTarget::Unit(target));
    let start = w.current_tick;
    run(&mut w, "dot", ctx);
    let mut hits = Vec::new();
    let mut last = hp(&w, target);
    for _ in 0..80 {
        step(&mut w);
        let now = hp(&w, target);
        if now < last {
            hits.push(w.current_tick - start);
        }
        last = now;
    }
    assert_eq!(hits, vec![10, 20, 31, 41, 51, 61, 71]);
    assert_eq!(w.effects().persistents().len(), 0);

    // A unit anchor ends the persistent when the unit dies.
    run(&mut w, "dot", ctx);
    assert_eq!(w.effects().persistents().len(), 1);
    w.entities.get_mut(&target).expect("alive").hp = SimScalar::ZERO;
    step(&mut w);
    assert_eq!(w.effects().persistents().len(), 0);
}

#[test]
fn a_point_missile_impacts_after_ceil_distance_over_speed() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "missile", node: LaunchMissile((to: TargetPoint, speed: 1.0,
            impact: Search((radius: 0.5, child: Damage((amount: 5.0)))))))"#,
    );
    let caster = unit(&mut w, 0, 10, 10);
    let victim = unit(&mut w, 0, 12, 10);
    // Pull the victim's point 2.5 from the caster: 3 ticks of flight.
    w.entities.get_mut(&victim).expect("spawned").position = scalar_point("12.5", "10");
    let ctx = context(
        &w,
        Some(caster),
        EffectTarget::Point(scalar_point("12.5", "10")),
    );
    let before = hp(&w, victim);
    // A walk run between ticks is stepped first in the next tick's sub-step (a), as a cast
    // in that tick's phase 1 would be: that tick is the cast tick.
    let cast = w.current_tick + 1;
    run(&mut w, "missile", ctx);
    let mut impact = None;
    for _ in 0..6 {
        step(&mut w);
        if impact.is_none() && hp(&w, victim) < before {
            impact = Some(w.current_tick - cast);
        }
    }
    assert_eq!(impact, Some(3));
    assert_eq!(w.effects().missiles().len(), 0);
}

fn scalar_point(x: &str, y: &str) -> SimVec2 {
    SimVec2::new(scalar(x), scalar(y))
}

#[test]
fn a_homing_missile_is_dropped_when_its_target_dies() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "missile", node: LaunchMissile((to: TargetUnit, speed: 0.5,
            impact: Damage((amount: 5.0)))))"#,
    );
    let caster = unit(&mut w, 0, 10, 10);
    let target = unit(&mut w, 0, 20, 10);
    let ctx = context(&w, Some(caster), EffectTarget::Unit(target));
    run(&mut w, "missile", ctx);
    step(&mut w);
    assert_eq!(w.effects().missiles().len(), 1);
    w.entities.get_mut(&target).expect("alive").hp = SimScalar::ZERO;
    step(&mut w);
    assert_eq!(w.effects().missiles().len(), 0);
}

#[test]
fn spell_damage_takes_shields_first_ignores_armour_and_adds_attribute_bonus() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "hit", node: Damage((amount: 10.0,
            attribute_bonus: [(Light, 1.0), (Armored, 1.0), (Biological, 1.0), (Mechanical, 1.0),
                (Psionic, 1.0), (Massive, 1.0), (Structure, 1.0), (Heroic, 1.0)]))),
           (name: "nothing", node: Damage((amount: 0.0)))"#,
    );
    let kind = support::archetype(&w, MoverType::Ground, true);
    let arch = w.catalog.get(kind).expect("listed").clone();
    let id = w
        .spawn_unit(kind, PlayerId(0), SimVec2::from_i32(10, 10))
        .expect("free");
    let bonus = SimScalar::from_i32(
        i32::try_from(arch.attributes.bits().count_ones()).expect("few attributes"),
    );
    let total = SimScalar::from_i32(10) + bonus;
    let (shields, life) = {
        let e = w.entities.get(&id).expect("spawned");
        (e.shields, e.hp)
    };
    let ctx = context(&w, None, EffectTarget::Unit(id));
    run(&mut w, "hit", ctx);
    let e = w.entities.get(&id).expect("alive");
    let to_shields = total.min(shields);
    assert_eq!(e.shields, shields - to_shields);
    assert_eq!(e.hp, life - (total - to_shields), "no armour reduction");
    assert_eq!(e.ticks_since_last_damage, 0);
    w.entities
        .get_mut(&id)
        .expect("alive")
        .ticks_since_last_damage = 9;
    run(&mut w, "nothing", ctx);
    assert_eq!(
        w.entities.get(&id).expect("alive").ticks_since_last_damage,
        9,
        "zero damage does not reset the damage timers"
    );
}

#[test]
fn a_status_inserts_refreshes_keeping_phase_and_first_context_then_expires() {
    let mut w = world(
        r#"(name: "storm", duration_ticks: 11.901786, period_ticks: Some(10.196429),
            initial_effect: Some("first"), periodic_effect: Some("tick"))"#,
        r#"(name: "apply", node: ApplyStatus((status: "storm"))),
           (name: "first", node: Damage((amount: 10.0))),
           (name: "tick", node: Damage((amount: 1.0))),
           (name: "remove", node: RemoveStatus((status: "storm")))"#,
    );
    let first = unit(&mut w, 0, 5, 5);
    let second = unit(&mut w, 0, 6, 5);
    let target = unit(&mut w, 0, 10, 10);
    let start = hp(&w, target);
    let ctx = context(&w, Some(first), EffectTarget::Unit(target));
    run(&mut w, "apply", ctx);
    assert_eq!(
        hp(&w, target),
        start - SimScalar::from_i32(10),
        "initial effect"
    );
    let applied = w.current_tick;
    let mut pulses = Vec::new();
    let mut last = hp(&w, target);
    for _ in 0..40 {
        step(&mut w);
        let elapsed = w.current_tick - applied;
        if elapsed == 5 || elapsed == 15 {
            // Refresh by another caster: no second initial damage, phase and context kept.
            let ctx = context(&w, Some(second), EffectTarget::Unit(target));
            run(&mut w, "apply", ctx);
        }
        let now = hp(&w, target);
        if now < last {
            pulses.push(elapsed);
        }
        last = now;
    }
    // Refreshed at 5 (to 16.9) and 15 (to 26.9): pulses at 10 and 20 only, then expiry at 27.
    assert_eq!(pulses, vec![10, 20]);
    assert_eq!(w.effects().statuses().len(), 0);
    let ctx = context(&w, Some(first), EffectTarget::Unit(target));
    run(&mut w, "apply", ctx);
    let ctx = context(&w, Some(second), EffectTarget::Unit(target));
    run(&mut w, "apply", ctx);
    let stored = w.effects().statuses()[0].context.caster;
    assert_eq!(stored, Some(first), "the first applier's context is kept");
    let ctx = context(&w, None, EffectTarget::Unit(target));
    run(&mut w, "remove", ctx);
    assert_eq!(w.effects().statuses().len(), 0);
}

#[test]
fn modify_vitals_changes_and_clamps() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "drain", node: ModifyVitals((energy: Some((change: -1000.0)),
            life: Some((change: 1000.0)))))"#,
    );
    let id = unit(&mut w, 0, 10, 10);
    w.entities.get_mut(&id).expect("spawned").hp = SimScalar::ONE;
    let ctx = context(&w, None, EffectTarget::Unit(id));
    run(&mut w, "drain", ctx);
    let e = w.entities.get(&id).expect("alive");
    assert_eq!(e.energy, SimScalar::ZERO);
    assert_eq!(e.hp, e.max_hp);
}

#[test]
fn cancel_orders_clears_queue_hold_and_target() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "cancel", node: CancelOrders((force_unburrow: true)))"#,
    );
    let id = unit(&mut w, 0, 10, 10);
    {
        let e = w.entities.get_mut(&id).expect("spawned");
        e.order_queue.push_back(UnitOrder::Move {
            target: SimVec2::from_i32(30, 30),
        });
        e.is_holding_position = true;
        e.current_target = Some(id);
        e.channeling_remaining_ticks = 5;
    }
    let ctx = context(&w, None, EffectTarget::Unit(id));
    run(&mut w, "cancel", ctx);
    let e = w.entities.get(&id).expect("alive");
    assert!(e.order_queue.is_empty() && !e.is_holding_position && e.current_target.is_none());
    assert_eq!(e.channeling_remaining_ticks, 5, "a mode channel continues");
}

#[test]
fn a_displacement_flies_linearly_lands_at_the_origin_and_runs_on_land() {
    let mut w = world(
        r#"(name: "held", duration_ticks: 100.0)"#,
        r#"(name: "pull", node: Set((children: [ApplyStatus((status: "held")),
            Displace((flight_ticks: 4, tether: true,
                on_land: Some(RemoveStatus((status: "held")))))])))"#,
    );
    let caster = unit(&mut w, 0, 10, 10);
    let target = unit(&mut w, 0, 18, 10);
    let mut ctx = context(&w, Some(caster), EffectTarget::Unit(target));
    ctx.origin = SimVec2::from_i32(14, 10);
    run(&mut w, "pull", ctx);
    let mut xs = Vec::new();
    for _ in 0..4 {
        step(&mut w);
        xs.push(w.entities.get(&target).expect("alive").position.x);
        if w.entities
            .get(&target)
            .expect("alive")
            .displacement
            .is_some()
        {
            assert_eq!(
                w.build_render_snapshot().links.len(),
                1,
                "the tether is shown"
            );
        }
    }
    let expected: Vec<_> = [17, 16, 15, 14]
        .into_iter()
        .map(SimScalar::from_i32)
        .collect();
    assert_eq!(xs, expected);
    assert!(w
        .entities
        .get(&target)
        .expect("alive")
        .displacement
        .is_none());
    assert!(
        w.effects().statuses().is_empty(),
        "on_land removed the status"
    );
}

#[test]
fn a_require_filter_skips_the_node_and_its_children() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "guarded", node: Set((require: Some((alliance: Enemy)),
            children: [Damage((amount: 10.0))])))"#,
    );
    let own = unit(&mut w, 0, 10, 10);
    let enemy = unit(&mut w, 1, 30, 30);
    let (own_hp, enemy_hp) = (hp(&w, own), hp(&w, enemy));
    let ctx = context(&w, None, EffectTarget::Unit(own));
    run(&mut w, "guarded", ctx);
    let ctx = context(&w, None, EffectTarget::Unit(enemy));
    run(&mut w, "guarded", ctx);
    assert_eq!(hp(&w, own), own_hp);
    assert_eq!(hp(&w, enemy), enemy_hp - SimScalar::from_i32(10));
}

#[test]
fn instances_created_in_phase_two_first_step_on_the_next_tick_in_id_order() {
    // The outer persistent's pulse at 1 creates the inner one; it pulses one tick later at
    // the earliest. Two persistents created together pulse in id order: damage, then halve.
    let mut w = world(
        r#"(name: "quick", duration_ticks: 50.0, period_ticks: Some(1.0),
            periodic_effect: Some("one"))"#,
        r#"(name: "outer", node: Persistent((anchor: TargetUnit, period_ticks: 1.0, count: 1,
            periodic: Some(Set((children: [ApplyStatus((status: "quick")),
                Persistent((anchor: TargetUnit, period_ticks: 1.0, count: 1,
                    periodic: Some(Damage((amount: 7.0)))))])))))),
           (name: "one", node: Damage((amount: 1.0))),
           (name: "a", node: Persistent((anchor: TargetUnit, period_ticks: 1.0, count: 1,
                periodic: Some(Damage((amount: 10.0)))))),
           (name: "b", node: Persistent((anchor: TargetUnit, period_ticks: 1.0, count: 1,
                periodic: Some(ModifyVitals((life: Some((change_fraction: -0.5))))))))"#,
    );
    let id = unit(&mut w, 0, 10, 10);
    let ctx = context(&w, None, EffectTarget::Unit(id));
    run(&mut w, "outer", ctx);
    let start = hp(&w, id);
    step(&mut w);
    assert_eq!(
        hp(&w, id),
        start,
        "the status and the inner persistent wait a tick"
    );
    assert_eq!(w.effects().persistents().len(), 1);
    step(&mut w);
    assert_eq!(
        hp(&w, id),
        start - SimScalar::from_i32(8),
        "inner 7, status pulse 1"
    );

    let mut w = world(
        NO_STATUSES,
        r#"(name: "a", node: Persistent((anchor: TargetUnit, period_ticks: 1.0, count: 1,
                periodic: Some(Damage((amount: 10.0)))))),
           (name: "b", node: Persistent((anchor: TargetUnit, period_ticks: 1.0, count: 1,
                periodic: Some(ModifyVitals((life: Some((change_fraction: -0.5))))))))"#,
    );
    let id = unit(&mut w, 0, 10, 10);
    let ctx = context(&w, None, EffectTarget::Unit(id));
    let start = hp(&w, id);
    run(&mut w, "a", ctx);
    run(&mut w, "b", ctx);
    let ids: Vec<_> = w.effects().persistents().iter().map(|p| p.id).collect();
    assert_eq!(ids, vec![1, 2]);
    assert!(matches!(w.effects().persistents()[0].anchor, PersistentAnchor::Unit(u) if u == id));
    step(&mut w);
    assert_eq!(
        hp(&w, id),
        (start - SimScalar::from_i32(10)) * SimScalar::HALF
    );
}

#[test]
fn a_point_persistent_that_searches_is_presented_as_an_area() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "zone", node: Persistent((anchor: TargetPoint, period_ticks: 10.2, count: 6,
            initial: Some(Use("hit")), periodic: Some(Use("hit"))))),
           (name: "hit", node: Search((radius: 1.5, child: Damage((amount: 1.0)))))"#,
    );
    let ctx = context(&w, None, EffectTarget::Point(SimVec2::from_i32(20, 20)));
    run(&mut w, "zone", ctx);
    let snapshot = w.build_render_snapshot();
    assert_eq!(snapshot.areas.len(), 1);
    assert!((snapshot.areas[0].radius - 1.5).abs() < f32::EPSILON);
    assert_eq!(snapshot.areas[0].total_ticks, 61);
}

#[test]
fn an_effect_ability_cast_pays_and_starts_its_walk() {
    let mut w = world(
        NO_STATUSES,
        r#"(name: "bolt", node: Damage((amount: 10.0)))"#,
    );
    let caster = unit(&mut w, 0, 10, 10);
    let target = unit(&mut w, 1, 12, 10);
    let kind = w.entities.get(&caster).expect("spawned").unit_type_id;
    let arch = w.catalog.get_mut(kind).expect("listed");
    arch.abilities.push(AbilitySpec {
        name: "Bolt".into(),
        target_kind: AbilityTargetKind::TargetEntity,
        energy_cost: SimScalar::ZERO,
        cooldown_ticks: 5,
        cast_range: SimScalar::from_i32(10),
        effect: AbilityEffectSpec::Effect("bolt".into()),
        leave_name: None,
        cast_backswing_ticks: 0,
        single_caster: true,
        target_filter: None,
        origin: None,
    });
    let index = u8::try_from(arch.abilities.len() - 1).expect("few abilities");
    let before = hp(&w, target);
    assert!(w.execute_ability(caster, index, None, Some(target)));
    assert_eq!(hp(&w, target), before - SimScalar::from_i32(10));
    assert_eq!(effect(&w, "bolt"), sim_types::EffectId(0));
}
