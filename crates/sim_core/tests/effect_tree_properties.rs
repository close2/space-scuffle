//! Property tests of the effect-tree engine (plan `2026-11-10-spellcaster-mechanics` §11):
//! random valid trees over random worlds of up to 64 units are deterministic and stay within
//! the work bound of D-17, and every generated invalid tree is refused with the matching
//! typed error.

#![allow(clippy::expect_used)]

#[path = "support/effect_world.rs"]
mod support;

use proptest::prelude::*;
use sim_core::catalog::{CatalogError, SpellCatalog, MAX_TREE_NODES};
use sim_core::SimCoreError;
use sim_types::{EffectTarget, SimVec2};
use support::{context, step, world};

const STATUSES: &str = r#"(name: "s", duration_ticks: 6.0, period_ticks: Some(2.0),
    periodic_effect: Some("pulse"))"#;
const PULSE: &str = r#"(name: "pulse", node: Damage((amount: 1.0)))"#;

fn leaf() -> impl Strategy<Value = String> {
    prop_oneof![
        (0_u8..20).prop_map(|a| format!("Damage((amount: {a}.0))")),
        (-5_i8..5).prop_map(|c| format!("ModifyVitals((life: Some((change: {c}.0))))")),
        Just(r#"ApplyStatus((status: "s"))"#.to_owned()),
        Just("CancelOrders(())".to_owned()),
    ]
}

/// A node that acts on a unit: a leaf or a set of one to three leaves.
fn body() -> impl Strategy<Value = String> {
    prop_oneof![
        leaf(),
        prop::collection::vec(leaf(), 1..=3)
            .prop_map(|children| format!("Set((children: [{}]))", children.join(", "))),
    ]
}

fn search() -> impl Strategy<Value = String> {
    let alliance = prop_oneof![
        Just("Any"),
        Just("AnyButSelf"),
        Just("Enemy"),
        Just("Friendly")
    ];
    (1_u8..=6, alliance, body()).prop_map(|(radius, alliance, child)| {
        format!("Search((radius: {radius}.0, filter: (alliance: {alliance}), child: {child}))")
    })
}

/// A root and whether it acts on a unit (else on a point).
fn root() -> impl Strategy<Value = (String, bool)> {
    prop_oneof![
        body().prop_map(|b| (b, true)),
        search().prop_map(|s| (s, false)),
        (search(), search(), 1_u8..=5, 1_u8..=4).prop_map(|(i, p, period, count)| (
            format!(
                "Persistent((anchor: TargetPoint, initial: Some({i}), periodic: Some({p}), \
                 period_ticks: {period}.0, count: {count}))"
            ),
            false
        )),
        (search(), 1_u8..=20).prop_map(|(impact, speed)| (
            format!(
                "LaunchMissile((to: TargetPoint, speed: {}.{}, impact: {impact}))",
                speed / 10,
                speed % 10
            ),
            false
        )),
    ]
}

fn units() -> impl Strategy<Value = Vec<(u8, i32, i32)>> {
    prop::collection::vec((0_u8..2, 4_i32..40, 4_i32..40), 1..=64)
}

/// Runs `tree` on a world of `layout`; returns the checksum after 30 ticks and the nodes the
/// first walk entered.
fn evaluate(tree: &str, unit_target: bool, layout: &[(u8, i32, i32)]) -> (u64, u64) {
    let mut w = world(
        STATUSES,
        &format!(r#"{PULSE}, (name: "root", node: {tree})"#),
    );
    let mut ids = Vec::new();
    for &(owner, x, y) in layout {
        if let Ok(id) = w.spawn_unit(
            support::archetype(&w, sim_types::MoverType::Ground, false),
            sim_types::PlayerId(owner),
            SimVec2::from_i32(x, y),
        ) {
            ids.push(id);
        }
    }
    let target = if unit_target {
        EffectTarget::Unit(ids[0])
    } else {
        EffectTarget::Point(SimVec2::from_i32(20, 20))
    };
    let ctx = context(&w, ids.first().copied(), target);
    let before = w.effect_node_visits();
    support::run(&mut w, "root", ctx);
    let visits = w.effect_node_visits() - before;
    for _ in 0..30 {
        step(&mut w);
    }
    (w.last_checksum(), visits)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn random_trees_are_deterministic_and_within_the_work_bound(
        (tree, unit_target) in root(),
        layout in units(),
    ) {
        let first = evaluate(&tree, unit_target, &layout);
        let second = evaluate(&tree, unit_target, &layout);
        prop_assert_eq!(first, second);
        let bound = u64::try_from(MAX_TREE_NODES * (layout.len() + 1)).expect("small");
        prop_assert!(first.1 <= bound, "{} visits above {}", first.1, bound);
    }

    #[test]
    fn invalid_trees_are_refused_with_the_matching_error((tree, _) in root(), defect in 0_u8..5) {
        let (bad, expected): (&str, fn(&CatalogError) -> bool) = match defect {
            0 => ("Search((radius: 0.0, child: Damage((amount: 1.0))))",
                  |e| matches!(e, CatalogError::InvalidSearch { .. })),
            1 => ("Set((children: []))", |e| matches!(e, CatalogError::EmptySet { .. })),
            2 => ("LaunchMissile((to: TargetPoint, speed: 100.0, impact: Damage((amount: 1.0))))",
                  |e| matches!(e, CatalogError::InvalidMissileSpeed { .. })),
            3 => ("Displace((flight_ticks: 0))",
                  |e| matches!(e, CatalogError::InvalidDisplacement { .. })),
            _ => (r#"ApplyStatus((status: "missing"))"#,
                  |e| matches!(e, CatalogError::UnknownStatus { .. })),
        };
        let effects = format!(
            r#"(effects: [{PULSE}, (name: "root", node: Set((children: [{tree}, {bad}])))])"#
        );
        let result = SpellCatalog::from_ron_strs(&format!("(statuses: [{STATUSES}])"), &effects);
        match result {
            Err(SimCoreError::SpellCatalog(error)) => prop_assert!(expected(&error), "{error:?}"),
            other => prop_assert!(false, "expected a catalog error, got {other:?}"),
        }
    }
}
