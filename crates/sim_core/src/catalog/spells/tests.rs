//! Unit tests of the spell catalog: one per [`CatalogError`] variant of plan D-15 and D-17, on
//! minimal scratch RON, plus deterministic flattening and the facts of the four planned trees.

#![allow(clippy::expect_used, clippy::too_many_lines)]

use super::*;
use crate::test_support::shipped_catalogs;
use sim_types::{AbilityEffectSpec, AbilityTargetKind, OriginPlacement};

/// The flattened catalog of `statuses` and `effects` (bodies of the two lists).
fn load(statuses: &str, effects: &str) -> Result<SpellCatalog, SimCoreError> {
    SpellCatalog::from_ron_strs(
        &format!("(statuses: [{statuses}])"),
        &format!("(effects: [{effects}])"),
    )
}

fn error(statuses: &str, effects: &str) -> CatalogError {
    match load(statuses, effects) {
        Err(SimCoreError::SpellCatalog(error)) => Some(error),
        _ => None,
    }
    .expect("invariant: the scratch catalog breaks a spell catalog rule")
}

const DOT: &str = r#"(name: "dot", duration_ticks: 11.9, period_ticks: Some(10.2),
    initial_effect: Some("hit"), periodic_effect: Some("hit"))"#;
const HIT: &str = r#"(name: "hit", node: Damage((amount: 10.0)))"#;

/// A chain of `depth` sets ending in a damage node, rooted at `name`.
fn chain(name: &str, depth: usize) -> String {
    let mut node = "Damage((amount: 1.0))".to_owned();
    for _ in 1..depth {
        node = format!("Set((children: [{node}]))");
    }
    format!(r#"(name: "{name}", node: {node})"#)
}

#[test]
fn the_shipped_files_load_with_the_four_spells() {
    let catalogs = shipped_catalogs();
    let spells = catalogs.units.spells();
    let root = spells
        .effect_id("psionic_storm")
        .expect("invariant: the shipped storm tree");
    let facts = spells.facts(root).expect("invariant: facts of every node");
    assert_eq!(facts.area_radius, Some(SimScalar::from_num(1.5)));
    assert_eq!(facts.targeting_radius, Some(SimScalar::from_num(1.5)));
    // Persistent, search, apply status, then the status's initial damage (plan D-17).
    assert_eq!(facts.sync_depth, 4);
    let abduct = spells
        .effect_id("abduct")
        .and_then(|root| spells.facts(root))
        .expect("invariant: the shipped Abduct tree");
    assert!(abduct.needs_origin, "Abduct lands at its origin (plan D-6)");
    assert_eq!(abduct.targeting_radius, None);
    let fungal = spells
        .effect_id("fungal_growth")
        .and_then(|root| spells.facts(root))
        .expect("invariant: the shipped Fungal Growth tree");
    // The ring shows the impact's search; a missile's impact is no area (plan D-19).
    assert_eq!(fungal.targeting_radius, Some(SimScalar::from_num(2.25)));
    assert_eq!(fungal.area_radius, None);
    assert!(!fungal.needs_origin);
    let cloud = spells
        .effect_id("blinding_cloud")
        .and_then(|root| spells.facts(root))
        .expect("invariant: the shipped Blinding Cloud tree");
    assert_eq!(cloud.area_radius, Some(SimScalar::from_num(2)));
    assert_eq!(cloud.targeting_radius, Some(SimScalar::from_num(2)));
    assert!(!cloud.needs_origin);
    assert_eq!(
        spells.status_count(),
        6,
        "psionic_storm, abducted, fungal_growth, fungal_growth_slow, blinding_cloud and \
         blinding_cloud_structure"
    );
}

#[test]
fn unknown_effect_and_status_keys_are_named_with_their_referrer() {
    assert_eq!(
        error(
            "",
            r#"(name: "a", node: Set((children: [Use("missing")])))"#
        ),
        CatalogError::UnknownEffect {
            name: "missing".into(),
            referenced_by: "a".into()
        }
    );
    assert_eq!(
        error("", r#"(name: "a", node: ApplyStatus((status: "nope")))"#),
        CatalogError::UnknownStatus {
            name: "nope".into(),
            referenced_by: "a".into()
        }
    );
    assert_eq!(
        error(
            "",
            r#"(name: "a", node: Search((radius: 1.0, filter: (without_status: Some("nope")),
                child: Damage((amount: 1.0)))))"#
        ),
        CatalogError::UnknownStatus {
            name: "nope".into(),
            referenced_by: "a".into()
        }
    );
    assert_eq!(
        error(
            r#"(name: "s", duration_ticks: 5.0, initial_effect: Some("gone"))"#,
            ""
        ),
        CatalogError::UnknownEffect {
            name: "gone".into(),
            referenced_by: "s".into()
        }
    );
    assert_eq!(
        error("", r#"(name: "alias", node: Use("gone"))"#),
        CatalogError::UnknownEffect {
            name: "gone".into(),
            referenced_by: "alias".into()
        }
    );
}

#[test]
fn duplicate_names_are_refused() {
    assert_eq!(
        error("", &format!("{HIT}, {HIT}")),
        CatalogError::DuplicateEffect { name: "hit".into() }
    );
    let status = r#"(name: "s", duration_ticks: 1.0)"#;
    assert_eq!(
        error(&format!("{status}, {status}"), ""),
        CatalogError::DuplicateStatus { name: "s".into() }
    );
}

#[test]
fn cycles_through_use_aliases_and_statuses_are_refused() {
    let through_use = error(
        "",
        r#"(name: "a", node: Set((children: [Use("b")]))),
           (name: "b", node: Set((children: [Use("a")])))"#,
    );
    assert!(
        matches!(&through_use, CatalogError::EffectCycle { path } if path.first() == path.last()),
        "{through_use:?}"
    );
    let aliases = error(
        "",
        r#"(name: "a", node: Use("b")), (name: "b", node: Use("a"))"#,
    );
    assert!(
        matches!(aliases, CatalogError::EffectCycle { .. }),
        "{aliases:?}"
    );
    let through_status = error(
        r#"(name: "s", duration_ticks: 5.0, period_ticks: Some(2.0), periodic_effect: Some("again"))"#,
        r#"(name: "again", node: ApplyStatus((status: "s")))"#,
    );
    assert_eq!(
        through_status,
        CatalogError::EffectCycle {
            path: vec!["again".into(), "again".into()]
        }
    );
}

#[test]
fn depth_nine_is_refused_and_eight_accepted() {
    load("", &chain("deep", MAX_EFFECT_DEPTH)).expect("invariant: depth 8 is allowed");
    assert_eq!(
        error("", &chain("deep", MAX_EFFECT_DEPTH + 1)),
        CatalogError::EffectTreeTooDeep {
            root: "deep".into(),
            depth: MAX_EFFECT_DEPTH + 1
        }
    );
}

#[test]
fn a_status_initial_effect_counts_toward_depth_and_a_periodic_one_does_not() {
    let status_initial = r#"(name: "s", duration_ticks: 5.0, initial_effect: Some("deep"))"#;
    let tree = format!(
        r#"{}, (name: "root", node: Set((children: [ApplyStatus((status: "s"))])))"#,
        chain("deep", MAX_EFFECT_DEPTH - 1)
    );
    assert_eq!(
        error(status_initial, &tree),
        CatalogError::EffectTreeTooDeep {
            root: "root".into(),
            depth: MAX_EFFECT_DEPTH + 1
        }
    );
    let status_periodic = r#"(name: "s", duration_ticks: 5.0, period_ticks: Some(1.0), periodic_effect: Some("deep"))"#;
    load(status_periodic, &tree).expect("invariant: a periodic effect starts its own walk");
}

#[test]
fn set_width_is_bounded() {
    assert_eq!(
        error("", r#"(name: "a", node: Set((children: [])))"#),
        CatalogError::EmptySet { effect: "a".into() }
    );
    let nine = ["Damage((amount: 1.0))"; MAX_SET_CHILDREN + 1].join(", ");
    assert_eq!(
        error(
            "",
            &format!(r#"(name: "a", node: Set((children: [{nine}])))"#)
        ),
        CatalogError::SetTooWide {
            effect: "a".into(),
            len: MAX_SET_CHILDREN + 1
        }
    );
}

#[test]
fn a_root_reaching_sixty_five_nodes_is_too_large() {
    // Eight sets of eight leaves under one set: 1 + 8 + 64 = 73 distinct nodes.
    let leaves = ["Damage((amount: 1.0))"; 8].join(", ");
    let sets = vec![format!("Set((children: [{leaves}]))"); 8].join(", ");
    let tree = format!(r#"(name: "big", node: Set((children: [{sets}])))"#);
    assert_eq!(
        error("", &tree),
        CatalogError::EffectTreeTooLarge {
            root: "big".into(),
            nodes: 73
        }
    );
}

#[test]
fn the_arena_is_bounded() {
    // 4097 named single nodes exceed the arena.
    let effects: Vec<String> = (0..=MAX_EFFECT_NODES)
        .map(|i| format!(r#"(name: "n{i:05}", node: Damage((amount: 1.0)))"#))
        .collect();
    assert_eq!(
        error("", &effects.join(", ")),
        CatalogError::TooManyEffectNodes
    );
}

#[test]
fn a_search_under_a_search_is_nested() {
    let tree = r#"(name: "outer", node: Search((radius: 1.0,
        child: Set((children: [Search((radius: 1.0, child: Damage((amount: 1.0))))])))))"#;
    assert_eq!(
        error("", tree),
        CatalogError::NestedSearch {
            outer: "outer".into(),
            inner: "outer/2".into()
        }
    );
    // A search behind a missile's impact starts a new walk and is allowed.
    load(
        "",
        r#"(name: "ok", node: Search((radius: 1.0, child: LaunchMissile((to: TargetUnit,
            speed: 1.0, impact: Search((radius: 1.0, child: Damage((amount: 1.0)))))))))"#,
    )
    .expect("invariant: a missile impact is its own walk");
}

#[test]
fn value_bounds_are_typed() {
    let search = |radius: &str, count: &str| {
        format!(
            r#"(name: "s", node: Search((radius: {radius}, max_count: {count},
                child: Damage((amount: 1.0)))))"#
        )
    };
    for (radius, count) in [
        ("0.0", "None"),
        ("16.5", "None"),
        ("1.0", "Some(0)"),
        ("1.0", "Some(257)"),
    ] {
        assert!(matches!(
            error("", &search(radius, count)),
            CatalogError::InvalidSearch { .. }
        ));
    }
    load("", &search("16.0", "Some(256)")).expect("invariant: bounds are inclusive");
    let persistent = |period: &str, count: u16| {
        format!(
            r#"(name: "p", node: Persistent((anchor: TargetPoint, periodic: Some(Damage((amount: 1.0))),
                period_ticks: {period}, count: {count}, require: None)))"#
        )
    };
    for (period, count) in [("0.5", 3), ("2.0", 0), ("2.0", 1025)] {
        assert!(
            matches!(
                error("", &persistent(period, count)),
                CatalogError::InvalidPeriod { .. }
            ),
            "{period} {count}"
        );
    }
    assert_eq!(
        error(
            "",
            r#"(name: "p", node: Persistent((anchor: TargetPoint)))"#
        ),
        CatalogError::EmptyPersistent { effect: "p".into() }
    );
    assert!(matches!(
        error(
            r#"(name: "s", duration_ticks: 5.0, period_ticks: Some(0.9), periodic_effect: Some("hit"))"#,
            HIT
        ),
        CatalogError::InvalidPeriod { .. }
    ));
    assert!(matches!(
        error(
            r#"(name: "s", duration_ticks: 5.0, period_ticks: Some(2.0))"#,
            ""
        ),
        CatalogError::InvalidPeriod { .. }
    ));
    for speed in ["0.009", "16.5"] {
        assert_eq!(
            error(
                "",
                &format!(
                    r#"(name: "m", node: LaunchMissile((to: TargetPoint, speed: {speed},
                    impact: Search((radius: 1.0, child: Damage((amount: 1.0)))))))"#
                )
            ),
            CatalogError::InvalidMissileSpeed { effect: "m".into() }
        );
    }
    for flight in [0, 251] {
        assert_eq!(
            error(
                "",
                &format!(r#"(name: "d", node: Displace((flight_ticks: {flight})))"#)
            ),
            CatalogError::InvalidDisplacement { effect: "d".into() }
        );
    }
    assert_eq!(
        error("", r#"(name: "d", node: Damage((amount: -1.0)))"#),
        CatalogError::NegativeDamage { effect: "d".into() }
    );
    assert_eq!(
        error(
            "",
            r#"(name: "d", node: Damage((amount: 1.0, attribute_bonus: [(Armored, -2.0)])))"#
        ),
        CatalogError::NegativeDamage { effect: "d".into() }
    );
    for vitals in ["()", "(life: Some((change_fraction: 1.5)))"] {
        assert!(matches!(
            error("", &format!(r#"(name: "v", node: ModifyVitals({vitals}))"#)),
            CatalogError::InvalidVitalChange { .. }
        ));
    }
    for status in [
        r#"(name: "s", duration_ticks: 0.0)"#,
        r#"(name: "s", duration_ticks: 1.0, move_speed_factor: 1.5)"#,
        r#"(name: "s", duration_ticks: 1.0, range_floor: -1.0)"#,
    ] {
        assert!(matches!(
            error(status, ""),
            CatalogError::InvalidStatus { .. }
        ));
    }
}

#[test]
fn a_unit_node_reached_with_a_point_is_a_mismatch() {
    // A point-anchored persistent hands a point to its damage child.
    let tree = r#"(name: "p", node: Persistent((anchor: TargetPoint,
        initial: Some(Damage((amount: 1.0))))))"#;
    let catalog = load("", tree).expect("invariant: valid until an ability roots it");
    let root = catalog.effect_id("p").expect("invariant: named");
    assert_eq!(
        catalog.check_ability_root(root, false, false, "caster/Spell"),
        Err(CatalogError::EffectTargetMismatch {
            effect: "p/1".into(),
            reached_from: "p".into()
        })
    );
    // A missile to a point gives its impact a point.
    let missile = r#"(name: "m", node: LaunchMissile((to: TargetPoint, speed: 1.0,
        impact: Damage((amount: 1.0)))))"#;
    let catalog = load("", missile).expect("invariant: valid until rooted");
    let root = catalog.effect_id("m").expect("invariant: named");
    assert!(matches!(
        catalog.check_ability_root(root, true, false, "caster/Spell"),
        Err(CatalogError::EffectTargetMismatch { .. })
    ));
    // The ability itself is the source of a point at the root.
    let hit = load("", HIT).expect("invariant: valid");
    assert_eq!(
        hit.check_ability_root(EffectId(0), false, false, "caster/Spell"),
        Err(CatalogError::EffectTargetMismatch {
            effect: "hit".into(),
            reached_from: "caster/Spell".into()
        })
    );
    // A status's effects always run on a unit; a point-only node there is fine, and a status
    // effect that needs a unit passes.
    load(DOT, HIT).expect("invariant: status effects run on the bearer");
}

#[test]
fn a_displacement_needs_an_origin_placing_ability() {
    let tree = r#"(name: "pull", node: Displace((flight_ticks: 18)))"#;
    let catalog = load("", tree).expect("invariant: valid");
    let root = catalog.effect_id("pull").expect("invariant: named");
    assert_eq!(
        catalog.check_ability_root(root, true, false, "viper/Abduct"),
        Err(CatalogError::DisplaceWithoutOrigin {
            effect: "pull".into(),
            ability: "viper/Abduct".into()
        })
    );
    catalog
        .check_ability_root(root, true, true, "viper/Abduct")
        .expect("invariant: with an origin the tree is valid");
    // Through a status's periodic effect.
    let via_status = load(
        r#"(name: "s", duration_ticks: 5.0, period_ticks: Some(1.0), periodic_effect: Some("pull"))"#,
        &format!(r#"{tree}, (name: "apply", node: ApplyStatus((status: "s")))"#),
    )
    .expect("invariant: valid");
    let apply = via_status.effect_id("apply").expect("invariant: named");
    assert!(matches!(
        via_status.check_ability_root(apply, true, false, "x/y"),
        Err(CatalogError::DisplaceWithoutOrigin { .. })
    ));
}

#[test]
fn an_effect_ability_must_target_a_point_or_a_unit() {
    let mut units = shipped_catalogs().units;
    // The shipped spells' casters name shipped effects the scratch tables lack.
    let ids: Vec<_> = units.archetypes().map(|a| a.unit_type_id).collect();
    for id in ids {
        let archetype = units.get_mut(id).expect("invariant: listed");
        archetype
            .abilities
            .retain(|ability| !matches!(ability.effect, AbilityEffectSpec::Effect(_)));
    }
    let spells = load("", HIT).expect("invariant: valid");
    let id = units
        .archetypes()
        .find(|a| !a.abilities.is_empty())
        .map(|a| a.unit_type_id)
        .expect("invariant: the shipped catalog has an ability");
    let archetype = units.get_mut(id).expect("invariant: listed");
    let slug = archetype.slug.clone();
    let ability = &mut archetype.abilities[0];
    let name = ability.name.clone();
    ability.effect = AbilityEffectSpec::Effect("hit".into());
    ability.target_kind = AbilityTargetKind::ToggleMode;
    ability.leave_name = None;
    assert_eq!(
        units.set_spell_catalog(spells.clone()),
        Err(SimCoreError::SpellCatalog(
            CatalogError::EffectAbilityNeedsTarget {
                ability: format!("{slug}/{name}")
            }
        ))
    );
    let ability = &mut units.get_mut(id).expect("listed").abilities[0];
    ability.target_kind = AbilityTargetKind::TargetEntity;
    units
        .set_spell_catalog(spells.clone())
        .expect("invariant: a unit-targeted damage root is valid");
    let ability = &mut units.get_mut(id).expect("listed").abilities[0];
    ability.effect = AbilityEffectSpec::Effect("absent".into());
    assert!(matches!(
        units.set_spell_catalog(spells.clone()),
        Err(SimCoreError::SpellCatalog(
            CatalogError::UnknownEffect { .. }
        ))
    ));
    let ability = &mut units.get_mut(id).expect("listed").abilities[0];
    ability.effect = AbilityEffectSpec::Effect("hit".into());
    ability.target_filter = Some(sim_types::TargetFilter {
        without_status: Some("absent".into()),
        ..sim_types::TargetFilter::default()
    });
    ability.origin = Some(OriginPlacement {
        forward_offset: SimScalar::ZERO,
        search_range: SimScalar::ONE,
    });
    assert!(matches!(
        units.set_spell_catalog(spells),
        Err(SimCoreError::SpellCatalog(
            CatalogError::UnknownStatus { .. }
        ))
    ));
}

/// The four planned spells of plan §5.2, as scratch data.
const PLAN_STATUSES: &str = r#"
    (name: "psionic_storm", duration_ticks: 11.901786, period_ticks: Some(10.196429),
        initial_effect: Some("psionic_storm_damage_initial"), periodic_effect: Some("psionic_storm_damage")),
    (name: "abducted", duration_ticks: 35.714286, stun: true, reveal: true,
        suppress_collision: true, suppress_boarding: true),
    (name: "fungal_growth", duration_ticks: 71.428571, period_ticks: Some(4.464286),
        periodic_effect: Some("fungal_growth_damage"), reveal: true),
    (name: "fungal_growth_slow", duration_ticks: 71.428571, move_speed_factor: 0.25,
        suppress_boarding: true, suppress_blink: true),
    (name: "blinding_cloud", duration_ticks: 11.160714, ranged_weapon_range_delta: -20.0),
    (name: "blinding_cloud_structure", duration_ticks: 11.160714, weapon_range_delta: -20.0,
        suppress_detection: true),
"#;

const PLAN_EFFECTS: &str = r#"
    (name: "psionic_storm", node: Persistent((anchor: TargetPoint,
        initial: Some(Use("psionic_storm_search")), periodic: Some(Use("psionic_storm_search")),
        period_ticks: 10.2, count: 6))),
    (name: "psionic_storm_search", node: Search((radius: 1.5,
        filter: (alliance: Any, planes: [Ground, Air], structures: Exclude),
        child: ApplyStatus((status: "psionic_storm"))))),
    (name: "psionic_storm_damage_initial", node: Damage((amount: 10.0, responses: (acquire: true, flee: true)))),
    (name: "psionic_storm_damage", node: Damage((amount: 10.0))),
    (name: "abduct", node: LaunchMissile((to: TargetUnit, speed: 1.05, view: Tether,
        impact: Set((children: [
            CancelOrders((force_unburrow: true)),
            ApplyStatus((status: "abducted")),
            Persistent((anchor: TargetUnit, periodic: Some(Displace((to: Origin, flight_ticks: 18,
                tether: true, on_land: Some(RemoveStatus((status: "abducted")))))),
                period_ticks: 1.785714, count: 1)),
        ]))))),
    (name: "fungal_growth", node: LaunchMissile((to: TargetPoint, speed: 0.672, view: Projectile,
        impact: Search((radius: 2.25, filter: (alliance: Enemy, planes: [Ground, Air], structures: Exclude),
            child: Set((children: [
                ApplyStatus((status: "fungal_growth")),
                ApplyStatus((status: "fungal_growth_slow")),
                Damage((amount: 0.0, responses: (acquire: true, flee: true))),
            ]))))))),
    (name: "fungal_growth_damage", node: Damage((amount: 1.5625))),
    (name: "blinding_cloud", node: Persistent((anchor: TargetPoint,
        initial: Some(Use("blinding_cloud_search")), periodic: Some(Use("blinding_cloud_search")),
        period_ticks: 8.928571, count: 15))),
    (name: "blinding_cloud_search", node: Search((radius: 2.0,
        filter: (alliance: AnyButSelf, planes: [Ground], structures: Include),
        child: Set((children: [
            ApplyStatus((status: "blinding_cloud", require: Some((structures: Exclude)))),
            ApplyStatus((status: "blinding_cloud_structure", require: Some((structures: Only)))),
        ]))))),
"#;

#[test]
fn the_four_planned_trees_load_with_their_facts() {
    let catalog = load(PLAN_STATUSES, PLAN_EFFECTS).expect("invariant: plan §5.2 is valid");
    let facts = |name: &str| {
        catalog
            .facts(catalog.effect_id(name).expect("invariant: named"))
            .expect("invariant: in the arena")
    };
    let scalar = |text: &str| ron::from_str::<SimScalar>(text).expect("invariant: a number");
    let storm = facts("psionic_storm");
    assert_eq!(
        storm.sync_depth, 4,
        "Persistent → Search → ApplyStatus → Damage"
    );
    assert_eq!(storm.area_radius, Some(scalar("1.5")));
    assert_eq!(storm.targeting_radius, Some(scalar("1.5")));
    assert!(!storm.needs_origin);
    let fungal = facts("fungal_growth");
    assert_eq!(fungal.sync_depth, 1, "the impact is its own walk");
    assert_eq!(fungal.area_radius, None);
    assert_eq!(fungal.targeting_radius, Some(scalar("2.25")));
    let cloud = facts("blinding_cloud");
    assert_eq!(cloud.area_radius, Some(scalar("2.0")));
    assert_eq!(cloud.targeting_radius, Some(scalar("2.0")));
    let abduct = facts("abduct");
    assert!(abduct.needs_origin);
    assert_eq!(abduct.targeting_radius, None);
    for (name, unit) in [
        ("psionic_storm", false),
        ("fungal_growth", false),
        ("blinding_cloud", false),
        ("abduct", true),
    ] {
        let root = catalog.effect_id(name).expect("invariant: named");
        catalog
            .check_ability_root(root, unit, name == "abduct", name)
            .expect("invariant: the plan's roots reach every node with the right kind");
    }
    // Abduct's delay persistent presents the tether while it waits.
    let delay = (0..catalog.node_count())
        .map(|i| EffectId(u16::try_from(i).expect("small")))
        .find(|id| {
            matches!(
                catalog.node(*id).map(|n| n.kind),
                Some(EffectNode::Persistent {
                    anchor: EffectAnchor::TargetUnit,
                    ..
                })
            )
        })
        .expect("invariant: Abduct has a unit-anchored persistent");
    assert!(catalog.facts(delay).is_some_and(|f| f.reaches_tether));
}

#[test]
fn ids_follow_names_whatever_the_file_order() {
    let forward = load(PLAN_STATUSES, PLAN_EFFECTS).expect("invariant: valid");
    let mut entries: Vec<&str> = PLAN_EFFECTS.split("\n    (name:").collect();
    let head = entries.remove(0);
    entries.reverse();
    let reversed_effects = format!("{head}\n    (name:{}", entries.join("\n    (name:"));
    let mut statuses: Vec<&str> = PLAN_STATUSES.split("\n    (name:").collect();
    let status_head = statuses.remove(0);
    statuses.reverse();
    let reversed_statuses = format!("{status_head}\n    (name:{}", statuses.join("\n    (name:"));
    let reversed = load(&reversed_statuses, &reversed_effects).expect("invariant: valid");
    assert_eq!(forward, reversed);
    assert_eq!(forward.effect_id("abduct"), Some(EffectId(0)), "name order");
    assert_eq!(forward.status_id("abducted"), Some(StatusId(0)));
    assert_eq!(
        forward.label(EffectId(9)),
        Some("abduct/1"),
        "inline after named"
    );
}

#[test]
fn a_named_alias_resolves_to_its_target_and_takes_no_id() {
    let catalog = load("", &format!(r#"{HIT}, (name: "alias", node: Use("hit"))"#))
        .expect("invariant: valid");
    assert_eq!(catalog.effect_id("alias"), catalog.effect_id("hit"));
    assert_eq!(catalog.node_count(), 1);
}
