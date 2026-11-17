//! The weapons' own looks and the spell missiles' looks through the effect system's public API
//! (plan `2026-11-09-effect-visualization` WP11, Amendment 24), on the shipped vocabulary and
//! the shipped units' presentation blocks.
//!
//! # Responsibility
//! - A tracer's, a beam's and a projectile's event asks for its weapon's `travel` effect along
//!   a line from where the shot leaves its unit's bubble to where it lands, binding its length
//!   and flight ([`travel_request`]); a melee strike, a splash and a death ask for none.
//! - A projectile's riding card flies its path for exactly its flight, and the effect ends with
//!   the flight; two shots along one path are two effects.
//! - The legacy combat card of an event is left out exactly when its unit names its weapon's
//!   looks ([`replaces_legacy`]); every shipped death has a look.
//! - A spell missile holds its look while it is published, and its impact is laid where the
//!   simulation ran it (its `SpellImpact` event, plan `2026-11-17-effect-snapshot-identity`
//!   D-6), bound to the radius and the status duration the spell catalog gives its ability; an
//!   enemy's impact the viewer cannot see is not laid.
//!
//! # Invariants verified
//! As listed; no unit, weapon or effect is named in Rust except through the shipped data
//! (charter C5): the units are found by what their blocks name.

#![allow(clippy::expect_used, clippy::panic, clippy::float_cmp)]

use bevy::math::{Vec2, Vec3};
use client::effects::areas::{
    keep_missiles_seen, lay_spell_impact_seen, AreaLooks, AreaSight, Keep,
};
use client::effects::catalog::schema::{Binding, EventKind, Priority};
use client::effects::catalog::{EffectCatalog, EffectId, UnitEffectTables, EFFECTS_CATALOG_PATH};
use client::effects::runtime::EffectRuntime;
use client::effects::systems::{replaces_legacy, spawn_request, travel_request};
use client::ResolvedAssetsRoot;
use sim_core::catalog::{UnitArchetype, UnitCatalog};
use sim_types::snapshot::CombatEventVisual;
use sim_types::{
    AbilitySource, EffectSource, PlayerId, SpellMissileView, UnitTypeId, WeaponKey, WeaponSlot,
};

/// The shipped vocabulary, units and their tables.
fn shipped() -> (EffectCatalog, UnitCatalog, UnitEffectTables) {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = UnitEffectTables::build(&units, &catalog).expect("every block resolves");
    (catalog, units, tables)
}

/// The first archetype whose block names, for a weapon in the primary slot of the default
/// stance, a travel effect whose kind and layers satisfy `wanted`, and that effect's id.
fn unit_with_travel(
    catalog: &EffectCatalog,
    units: &UnitCatalog,
    wanted: impl Fn(&client::effects::catalog::Effect) -> bool,
) -> (UnitTypeId, f32, EffectId) {
    units
        .archetypes()
        .find_map(|archetype| {
            let entry = archetype
                .effects
                .weapons
                .iter()
                .find(|entry| entry.weapon == WeaponSlot::Primary && entry.mode.is_none())?;
            let id = catalog.effect_id(entry.travel.as_ref()?)?;
            wanted(catalog.effect(id)?).then(|| {
                (
                    archetype.unit_type_id,
                    archetype.collision_radius.to_f32(),
                    id,
                )
            })
        })
        .expect("a shipped unit names such a travel")
}

/// Whether `effect` has a layer that lasts the shot's flight.
fn flies(effect: &client::effects::catalog::Effect) -> bool {
    effect
        .layers
        .iter()
        .any(|layer| layer.lasts == Some(Binding::FlightTime))
}

/// The primary weapon of `unit` in the default stance.
fn primary(unit: UnitTypeId) -> EffectSource {
    EffectSource {
        unit_type: unit,
        weapon: WeaponKey {
            slot: WeaponSlot::Primary,
            mode: None,
        },
    }
}

#[test]
fn an_instant_shot_asks_for_its_travel_from_the_bubble_to_the_impact() {
    let (catalog, units, tables) = shipped();
    let (unit, radius, effect) = unit_with_travel(&catalog, &units, |effect| !flies(effect));
    let event = CombatEventVisual::HitscanTracer {
        from: [2.0, 3.0, 0.5],
        to: [10.0, 3.0, 0.5],
        owner: PlayerId(0),
        source: primary(unit),
        ends: None,
    };
    let request = travel_request(&event, &tables, &catalog).expect("the unit names a travel");
    assert_eq!(request.effect, effect);
    let line = request.line.expect("a shot has a line");
    // Bevy world space: map (x, y, altitude) is (x, altitude, y).
    assert!(
        (line.from - Vec3::new(2.0 + radius, 0.5, 3.0)).length() < 1e-4,
        "the shot leaves the bubble's surface: {:?}",
        line.from
    );
    assert!((line.to - Vec3::new(10.0, 0.5, 3.0)).length() < 1e-4);
    assert_eq!(request.anchor, line.from);
    assert!((request.bound.get(Binding::LineLength) - (8.0 - radius)).abs() < 1e-4);
    assert_eq!(request.bound.get(Binding::FlightTime), 0.0);
    assert_eq!(
        catalog.effect(effect).map(|e| e.event),
        Some(EventKind::Shot)
    );
}

#[test]
fn a_projectile_flies_its_path_for_its_flight_and_ends_with_it() {
    let (catalog, units, tables) = shipped();
    let (unit, _, effect) = unit_with_travel(&catalog, &units, flies);
    let flight = 0.8;
    let event = CombatEventVisual::ProjectileVisual {
        pos: [4.0, 4.0, 0.5],
        vel: [5.0, 0.0, 0.0],
        owner: PlayerId(0),
        duration_secs: Some(flight),
        source: primary(unit),
        ends: None,
    };
    let request = travel_request(&event, &tables, &catalog).expect("the unit names a travel");
    assert_eq!(request.effect, effect);
    let line = request.line.expect("a shot has a line");
    assert!(
        (line.to - Vec3::new(8.0, 0.5, 4.0)).length() < 1e-4,
        "{:?}",
        line.to
    );
    assert_eq!(request.bound.get(Binding::FlightTime), flight);
    let mut runtime = EffectRuntime::new(&catalog);
    // Only the `Critical` layers: the body that rides the path.
    runtime.only = Some(Priority::Critical);
    runtime
        .spawn(&catalog, &request, 0.0)
        .expect("the pool takes it");
    for (age, share) in [(0.2_f64, 0.25_f32), (0.4, 0.5), (0.6, 0.75)] {
        runtime.build_frame(&catalog, age, None);
        let expected = line.from + (line.to - line.from) * share;
        assert!(
            runtime.frame.sorted.iter().any(|card| {
                let centre = Vec3::from_array(card.center);
                Vec2::new(centre.x - expected.x, centre.z - expected.z).length() < 1e-3
            }),
            "at {age} s the body flies over {expected:?}: {:?}",
            runtime
                .frame
                .sorted
                .iter()
                .map(|card| card.center)
                .collect::<Vec<_>>()
        );
    }
    runtime.build_frame(&catalog, f64::from(flight) + 0.01, None);
    assert!(
        runtime.frame.sorted.is_empty(),
        "the body is gone once it lands"
    );
    assert_eq!(
        runtime.pool.live_total(),
        0,
        "the effect ends with its flight"
    );
}

#[test]
fn two_shots_along_one_path_are_two_effects() {
    let (catalog, units, tables) = shipped();
    let (unit, _, _) = unit_with_travel(&catalog, &units, |effect| !flies(effect));
    let event = CombatEventVisual::HitscanTracer {
        from: [2.0, 3.0, 0.5],
        to: [10.0, 3.0, 0.5],
        owner: PlayerId(0),
        source: primary(unit),
        ends: None,
    };
    let request = travel_request(&event, &tables, &catalog).expect("a travel");
    let mut runtime = EffectRuntime::new(&catalog);
    assert!(runtime.spawn(&catalog, &request, 0.0).is_some());
    assert!(
        runtime.spawn(&catalog, &request, 0.0).is_some(),
        "the second shot is not merged into the first"
    );
    assert_eq!(runtime.pool.live_total(), 2);
}

#[test]
fn a_melee_strike_asks_for_its_impact_only() {
    let (catalog, units, tables) = shipped();
    let melee: &UnitArchetype = units
        .archetypes()
        .find(|archetype| {
            archetype.weapon.as_ref().is_some_and(|weapon| {
                weapon.visual_kind == sim_types::WeaponVisualKind::MeleeSlash
                    && weapon.projectile_speed.is_none()
                    && weapon.splash.is_none()
            })
        })
        .expect("a shipped melee unit");
    let event = CombatEventVisual::MeleeSlashVisual {
        pos: [5.0, 5.0, 0.5],
        angle_rad: 0.4,
        radius: 0.5,
        owner: PlayerId(0),
        source: primary(melee.unit_type_id),
        ends: None,
    };
    assert!(travel_request(&event, &tables, &catalog).is_none());
    let impact = spawn_request(&event, &tables, &catalog).expect("the strike has a look");
    assert_eq!(
        catalog.effect(impact.effect).map(|effect| effect.event),
        Some(EventKind::Point)
    );
    assert!((impact.anchor - Vec3::new(5.0, 0.5, 5.0)).length() < 1e-5);
}

#[test]
fn the_legacy_card_is_left_out_exactly_where_a_unit_names_its_looks() {
    let (catalog, units, tables) = shipped();
    let (unit, _, _) = unit_with_travel(&catalog, &units, |_| true);
    let tracer = |source| CombatEventVisual::HitscanTracer {
        from: [0.0; 3],
        to: [1.0, 0.0, 0.0],
        owner: PlayerId(0),
        source,
        ends: None,
    };
    assert!(replaces_legacy(Some(&tables), &tracer(primary(unit))));
    assert!(
        !replaces_legacy(Some(&tables), &tracer(EffectSource::default())),
        "a source no catalog entry names keeps the legacy card"
    );
    assert!(
        !replaces_legacy(None, &tracer(primary(unit))),
        "without the effect system nothing is replaced"
    );
    for archetype in units.archetypes() {
        let death = CombatEventVisual::BubbleShatter {
            pos: [0.0; 3],
            radius: 0.5,
            is_dome: false,
            owner: PlayerId(0),
            unit_type: archetype.unit_type_id,
        };
        assert!(
            replaces_legacy(Some(&tables), &death),
            "{} dies with a look of the effect system",
            archetype.slug
        );
        assert!(spawn_request(&death, &tables, &catalog).is_some());
    }
}

/// The first ability of a shipped unit whose block names a `Missile` look.
fn missile_ability(catalog: &EffectCatalog, units: &UnitCatalog) -> (AbilitySource, EffectId) {
    units
        .archetypes()
        .find_map(|archetype| {
            archetype.effects.abilities.iter().find_map(|(name, look)| {
                let id = catalog.effect_id(look)?;
                (catalog.effect(id)?.event == EventKind::Missile).then_some(())?;
                let index = archetype
                    .abilities
                    .iter()
                    .position(|spec| spec.name == *name)?;
                Some((
                    AbilitySource {
                        unit_type: archetype.unit_type_id,
                        ability: u8::try_from(index).ok()?,
                    },
                    id,
                ))
            })
        })
        .expect("a shipped caster names a missile look")
}

/// A missile of `source` with `id` at map point `(x, y)`.
fn missile(source: AbilitySource, id: u32, x: f32, y: f32) -> SpellMissileView {
    SpellMissileView {
        id,
        owner: PlayerId(1),
        position: [x, y, 0.0],
        source,
    }
}

#[test]
fn a_spell_missile_holds_its_look_and_lands_its_impact_where_it_ends() {
    let (catalog, units, tables) = shipped();
    let (source, look) = missile_ability(&catalog, &units);
    let impact = catalog
        .effect(look)
        .and_then(|effect| effect.impact)
        .expect("the missile lays an impact");
    let extent = tables.missile_extent(source);
    assert!(
        extent.radius > 0.0,
        "the spell searches a radius: {extent:?}"
    );
    assert!(
        extent.seconds > 0.0,
        "the spell applies a status: {extent:?}"
    );
    let mut runtime = EffectRuntime::new(&catalog);
    let mut looks = AreaLooks::default();
    let frames = [
        vec![missile(source, 7, 3.0, 3.0)],
        vec![missile(source, 7, 4.0, 3.0)],
        vec![missile(source, 7, 5.0, 3.0)],
        Vec::new(),
    ];
    // The simulation ran the impact at (5.8, 3.0): 0.8 of a tick's flight past the missile's
    // last published position, not the half step the client used to guess (B-124).
    let landing = CombatEventVisual::SpellImpact {
        pos: [5.8, 3.0, 0.0],
        missile: 7,
        owner: PlayerId(1),
        source,
    };
    for (index, frame) in frames.iter().enumerate() {
        let next = frames.get(index + 1).unwrap_or(frame);
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime: &mut runtime,
            #[expect(clippy::cast_precision_loss, reason = "a frame index")]
            now: index as f64 * 0.04,
            delta: 0.04,
        };
        keep_missiles_seen(
            &mut looks,
            (frame, next, 0.5),
            |_, _, _| AreaSight::Whole,
            &mut keep,
        );
        // The impact's event is shown on the frame the missile stops being published.
        if frame.is_empty() {
            assert!(
                lay_spell_impact_seen(&landing, |_, _, _| AreaSight::Whole, &mut keep).is_some()
            );
        }
        let held = usize::from(!frame.is_empty());
        assert_eq!(looks.missiles().0, held, "frame {index}");
    }
    assert_eq!(runtime.spawned_of(look), 1, "one look held for the flight");
    assert_eq!(runtime.spawned_of(impact), 1, "one impact where it ended");
    let landed = runtime
        .pool
        .iter()
        .find(|(_, instance)| instance.effect == impact)
        .map(|(_, instance)| *instance)
        .expect("the impact is live");
    // Laid where the event says (plan `2026-11-17-effect-snapshot-identity` D-6).
    assert!(
        (landed.anchor - Vec3::new(5.8, 0.0, 3.0)).length() < 1e-6,
        "{:?}",
        landed.anchor
    );
    assert_eq!(landed.bound.get(Binding::AreaRadius), extent.radius);
    assert_eq!(landed.bound.get(Binding::AreaTotal), extent.seconds);
}

#[test]
fn an_unseen_missile_is_not_drawn_and_lands_no_impact() {
    let (catalog, units, tables) = shipped();
    let (source, look) = missile_ability(&catalog, &units);
    let impact = catalog
        .effect(look)
        .and_then(|effect| effect.impact)
        .expect("an impact");
    let mut runtime = EffectRuntime::new(&catalog);
    let mut looks = AreaLooks::default();
    let flying = vec![missile(source, 3, 3.0, 3.0)];
    for (index, frame) in [&flying, &Vec::new()].into_iter().enumerate() {
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime: &mut runtime,
            #[expect(clippy::cast_precision_loss, reason = "a frame index")]
            now: index as f64 * 0.04,
            delta: 0.04,
        };
        keep_missiles_seen(
            &mut looks,
            (frame, frame, 0.0),
            |_, _, _| AreaSight::Hidden,
            &mut keep,
        );
        if frame.is_empty() {
            let landing = CombatEventVisual::SpellImpact {
                pos: [3.5, 3.0, 0.0],
                missile: 3,
                owner: PlayerId(1),
                source,
            };
            assert!(
                lay_spell_impact_seen(&landing, |_, _, _| AreaSight::Hidden, &mut keep).is_none()
            );
        }
    }
    assert_eq!(
        runtime.spawned_of(impact),
        0,
        "no impact where it cannot be seen"
    );
    runtime.build_frame(&catalog, 0.05, None);
    // Whatever of it is still eroding out is cut to nothing (`CLIP_NOTHING`): no point of the
    // world lies inside its clip window.
    let nothing = client::effects::areas::CLIP_NOTHING;
    assert!(
        runtime
            .frame
            .sorted
            .iter()
            .all(|card| card.clip_min >= nothing[2] && card.clip_axis == [nothing[0], nothing[1]]),
        "nothing of it is drawn"
    );
    assert!(runtime.frame.decals.is_empty(), "it lays no mark");
}

/// Review 1, E1-1: an instant shot's impact is laid when its slug arrives, never before. At 40 ms
/// frames, the impact's first drawn frame is not earlier than the frame the slug's head reaches
/// the target, less one.
/// The furthest a riding strip of `runtime`'s frame reaches along `line` from its start, world
/// units: the head of a slug, the far end of a beam. `None` when no strip is drawn.
fn strip_head(runtime: &EffectRuntime, line: client::effects::placement::WorldLine) -> Option<f32> {
    use client::effects::instances::{ORIENT_RIBBON, ORIENT_RIBBON_GROUND};
    let direction = (line.to - line.from).normalize_or_zero();
    runtime
        .frame
        .sorted
        .iter()
        .filter(|card| card.orient == ORIENT_RIBBON || card.orient == ORIENT_RIBBON_GROUND)
        .map(|card| {
            let head =
                Vec3::from(card.center) + Vec3::from(card.axis) * card.half_size * card.stretch;
            (head - line.from).dot(direction)
        })
        .reduce(f32::max)
}

/// Whether `runtime`'s frame draws anything at all.
fn draws(runtime: &EffectRuntime) -> bool {
    !runtime.frame.sorted.is_empty()
        || !runtime.frame.decals.is_empty()
        || !runtime.frame.lights.is_empty()
}

/// Review 2, E2-5: for every shipped instant travel, frame by frame on its own, the slug's head
/// (the furthest its riding strips reach) is found from the built frames; the impact, built on
/// its own, draws nothing before the frame in which that head reaches the shot's end (within
/// one frame's advance of the head). Independent of how the delay is computed.
#[test]
fn an_instant_shots_impact_waits_for_its_slug() {
    let (catalog, units, tables) = shipped();
    let step = 0.01_f64;
    let mut checked = Vec::new();
    for archetype in units.archetypes() {
        for entry in archetype
            .effects
            .weapons
            .iter()
            .filter(|entry| entry.mode.is_none())
        {
            let source = EffectSource {
                unit_type: archetype.unit_type_id,
                weapon: WeaponKey {
                    slot: entry.weapon,
                    mode: None,
                },
            };
            let (Some(travel), Some(_)) = (tables.travel(source), tables.impact(source)) else {
                continue;
            };
            if catalog.effect(travel).is_none_or(flies) {
                continue;
            }
            let event = CombatEventVisual::HitscanTracer {
                from: [2.0, 3.0, 0.5],
                to: [10.0, 3.0, 0.5],
                owner: PlayerId(0),
                source,
                ends: None,
            };
            let slug = travel_request(&event, &tables, &catalog).expect("an instant travel");
            let impact = spawn_request(&event, &tables, &catalog).expect("an impact");
            let line = slug.line.expect("a travel has its line");
            let length = line.from.distance(line.to);
            let mut alone = EffectRuntime::new(&catalog);
            alone.spawn(&catalog, &slug, 0.0).expect("the slug spawns");
            let mut struck = EffectRuntime::new(&catalog);
            struck
                .spawn(&catalog, &impact, 0.0)
                .expect("the impact spawns");
            let (mut heads, mut first_impact) = (Vec::new(), None);
            for frame in 0..80_u32 {
                let now = f64::from(frame) * step;
                alone.build_frame(&catalog, now, None);
                struck.build_frame(&catalog, now, None);
                heads.push(strip_head(&alone, line));
                if first_impact.is_none() && draws(&struck) {
                    first_impact = Some(frame);
                }
            }
            let advance = heads
                .windows(2)
                .filter_map(|pair| Some(pair[1]? - pair[0]?))
                .fold(0.0_f32, f32::max);
            let arrival = heads
                .iter()
                .position(|head| head.is_some_and(|head| head >= length - advance - 1e-3));
            let (Some(arrival), Some(first_impact)) = (arrival, first_impact) else {
                panic!(
                    "{} ({}): the slug reaches the end ({arrival:?}) and the impact draws \
                     ({first_impact:?})",
                    archetype.slug,
                    catalog
                        .effect(travel)
                        .map_or("", |effect| effect.name.as_str())
                );
            };
            assert!(
                first_impact as usize >= arrival,
                "{}: the impact draws at frame {first_impact}, before the slug's head reaches \
                 the end at frame {arrival}",
                archetype.slug
            );
            checked.push(format!("{}@{arrival}/{first_impact}", archetype.slug));
        }
    }
    assert!(
        checked.len() >= 3,
        "the shipped instant travels: {checked:?}"
    );
    eprintln!("instant travels (arrival frame / first impact frame, 10 ms): {checked:?}");
}
