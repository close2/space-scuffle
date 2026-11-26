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
    runtime.set_only(Some(Priority::Critical));
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
        runtime.pool().live_total(),
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
    assert_eq!(runtime.pool().live_total(), 2);
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
        .pool()
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
    assert!(
        runtime.current_decals(&catalog).is_empty(),
        "it lays no mark"
    );
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

/// Whether `runtime`'s frame draws anything at all (its decals as the reference ages them).
fn draws(runtime: &EffectRuntime, catalog: &EffectCatalog) -> bool {
    !runtime.frame.sorted.is_empty()
        || !runtime.current_decals(catalog).is_empty()
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
                if first_impact.is_none() && draws(&struck, &catalog) {
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

// --- Held beams (plan `2026-11-17-effect-snapshot-identity` WP7, D-4, B-126) -----------------

use client::effects::beams::{beam_line, BeamContext, HeldBeams, HELD_BEAMS};
use sim_types::{BubbleRenderInstance, EntityId, StrikeEnds};

/// The attack period of the test's beam attacker, ticks (the Void-Ray analogue's).
const BEAM_PERIOD_TICKS: u16 = 9;

/// A presented unit bubble of `unit` for entity `id` at map `(x, y)`, standing on the ground,
/// with an attack swing of [`BEAM_PERIOD_TICKS`].
fn beam_bubble(id: u32, unit: UnitTypeId, x: f32, y: f32) -> BubbleRenderInstance {
    use sim_types::{
        AnimState, AttackSwing, Concealment, ShapeKind, SilhouetteClass, StanceView, StatusFlags,
        WeaponVisualKind,
    };
    BubbleRenderInstance {
        entity_id: EntityId(id),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::HeavyHex,
        unit_type: unit,
        owner: PlayerId(u8::from(id >= 100)),
        world_pos: [x, y, 0.0],
        ground_z: 0.0,
        radius: 0.5,
        velocity: [0.0; 2],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: true,
        weapon_visual: WeaponVisualKind::LaserBeam,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        attack_swing: Some(AttackSwing {
            elapsed_ticks: 0,
            damage_point_ticks: 0,
            period_ticks: BEAM_PERIOD_TICKS,
        }),
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        concealment: Concealment::None,
        speed_boosted: false,
        stance: StanceView::default(),
        status: StatusFlags::default(),
        construction_hp_ratio: None,
        barrier_ratio: 1.0,
        housed: false,
    }
}

/// The first shipped unit whose primary weapon's travel look is sustained, its source and
/// that look. Found by data (C5).
fn beam_unit(catalog: &EffectCatalog, units: &UnitCatalog) -> (EffectSource, EffectId) {
    let (unit, _, effect) = unit_with_travel(catalog, units, |effect| effect.sustain.is_some());
    (primary(unit), effect)
}

/// One strike of `source` by `attacker` on `target`, as the simulation publishes it.
fn beam_strike(
    source: EffectSource,
    attacker: &BubbleRenderInstance,
    target: &BubbleRenderInstance,
) -> CombatEventVisual {
    let [ax, ay, _] = attacker.world_pos;
    let [tx, ty, _] = target.world_pos;
    CombatEventVisual::BeamVisual {
        from: [ax, ay, 0.5],
        to: [tx, ty, 0.5],
        is_lightning: false,
        owner: attacker.owner,
        source,
        ends: Some(StrikeEnds {
            attacker: attacker.entity_id,
            target: target.entity_id,
        }),
    }
}

/// Strikes `event` into `beams` at `now`, its line from its travel request.
fn strike_at(
    beams: &mut HeldBeams,
    event: &CombatEventVisual,
    presented: &[BubbleRenderInstance],
    (catalog, tables, runtime): (&EffectCatalog, &UnitEffectTables, &mut EffectRuntime),
    now: f64,
) -> bool {
    let line = travel_request(event, tables, catalog).and_then(|shot| shot.line);
    let mut context = BeamContext {
        catalog,
        tables,
        runtime,
        now,
        battle_now: now,
    };
    beams.strike(event, line, presented, &mut context)
}

/// Keeps `beams` on `presented` at `now`, without fog or camera.
fn keep_at(
    beams: &mut HeldBeams,
    presented: &[BubbleRenderInstance],
    (catalog, tables, runtime): (&EffectCatalog, &UnitEffectTables, &mut EffectRuntime),
    now: f64,
) {
    let mut context = BeamContext {
        catalog,
        tables,
        runtime,
        now,
        battle_now: now,
    };
    beams.keep(presented, None, None, &mut context);
}

/// The live line of the held beam `handle`.
fn held_line(
    runtime: &EffectRuntime,
    handle: client::effects::pool::EffectHandle,
) -> client::effects::placement::WorldLine {
    runtime
        .held(handle)
        .and_then(|instance| instance.line)
        .expect("a held beam has its line")
}

#[test]
fn a_stream_of_strikes_holds_one_beam() {
    let (catalog, units, tables) = shipped();
    let (source, effect) = beam_unit(&catalog, &units);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let presented = [attacker.clone(), target.clone()];
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let event = beam_strike(source, &attacker, &target);
    for strike in 0..10_u32 {
        let now = f64::from(strike) * 0.36;
        assert!(
            strike_at(
                &mut beams,
                &event,
                &presented,
                (&catalog, &tables, &mut runtime),
                now
            ),
            "a sustained beam's strike is taken"
        );
        keep_at(
            &mut beams,
            &presented,
            (&catalog, &tables, &mut runtime),
            now + 0.01,
        );
    }
    assert_eq!(beams.slots().len(), 1, "one beam for the stream");
    assert_eq!(runtime.spawned_of(effect), 1, "held once, never re-spawned");
    let slot = beams.slots()[0];
    assert!(runtime
        .held(slot.handle)
        .is_some_and(|i| i.released_at.is_none()));
    // A strike of a look that is not sustained, or without ends, is not taken.
    let mut no_ends = event.clone();
    if let CombatEventVisual::BeamVisual { ends, .. } = &mut no_ends {
        *ends = None;
    }
    assert!(!strike_at(
        &mut beams,
        &no_ends,
        &presented,
        (&catalog, &tables, &mut runtime),
        4.0
    ));
}

#[test]
fn the_beam_follows_both_bubbles() {
    let (catalog, units, tables) = shipped();
    let (source, _) = beam_unit(&catalog, &units);
    let mut attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let mut target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let presented = [attacker.clone(), target.clone()];
    strike_at(
        &mut beams,
        &beam_strike(source, &attacker, &target),
        &presented,
        (&catalog, &tables, &mut runtime),
        0.0,
    );
    let handle = beams.slots()[0].handle;
    let muzzle = tables.muzzle(source.unit_type);
    for (frame, (ax, tx, ty)) in [(4.5, 9.4, 4.6), (5.0, 9.8, 5.3), (5.2, 10.1, 6.0)]
        .into_iter()
        .enumerate()
    {
        attacker.world_pos = [ax, 4.0, 0.0];
        target.world_pos = [tx, ty, 0.0];
        let presented = [attacker.clone(), target.clone()];
        keep_at(
            &mut beams,
            &presented,
            (&catalog, &tables, &mut runtime),
            0.04 * f64::from(u8::try_from(frame).expect("three frames") + 1),
        );
        let line = held_line(&runtime, handle);
        let expected = beam_line(&attacker, &target, muzzle, None);
        assert!(
            line.from.distance(expected.from) < 1e-3,
            "frame {frame}: muzzle"
        );
        assert!(
            line.to.distance(expected.to) < 1e-3,
            "frame {frame}: target end"
        );
        // The ends lie on the bubbles: the muzzle on the attacker's glass, the end beyond the
        // target's, both at the bubbles' drawn heights.
        let centre = |bubble: &BubbleRenderInstance| {
            Vec3::new(bubble.world_pos[0], bubble.radius, bubble.world_pos[1])
        };
        assert!((line.from.distance(centre(&attacker)) - muzzle).abs() < 1e-3);
        let reach = target.radius + client::effects::struck::SURFACE_MARGIN;
        assert!((line.to.distance(centre(&target)) - reach).abs() < 1e-3);
    }
}

#[test]
fn a_retarget_moves_the_end_without_a_second_beam() {
    let (catalog, units, tables) = shipped();
    let (source, effect) = beam_unit(&catalog, &units);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let first = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let second = beam_bubble(101, source.unit_type, 4.0, 9.0);
    let presented = [attacker.clone(), first.clone(), second.clone()];
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    strike_at(
        &mut beams,
        &beam_strike(source, &attacker, &first),
        &presented,
        (&catalog, &tables, &mut runtime),
        0.0,
    );
    keep_at(
        &mut beams,
        &presented,
        (&catalog, &tables, &mut runtime),
        0.04,
    );
    strike_at(
        &mut beams,
        &beam_strike(source, &attacker, &second),
        &presented,
        (&catalog, &tables, &mut runtime),
        0.36,
    );
    keep_at(
        &mut beams,
        &presented,
        (&catalog, &tables, &mut runtime),
        0.4,
    );
    assert_eq!(beams.slots().len(), 1);
    assert_eq!(beams.slots()[0].target, second.entity_id);
    assert_eq!(runtime.spawned_of(effect), 1, "no second beam");
    let line = held_line(&runtime, beams.slots()[0].handle);
    let expected = beam_line(&attacker, &second, tables.muzzle(source.unit_type), None);
    assert!(
        line.to.distance(expected.to) < 1e-3,
        "the end moved to the new target"
    );
}

#[test]
fn the_beam_ends_when_the_target_is_gone() {
    let (catalog, units, tables) = shipped();
    let (source, _) = beam_unit(&catalog, &units);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let presented = [attacker.clone(), target.clone()];
    strike_at(
        &mut beams,
        &beam_strike(source, &attacker, &target),
        &presented,
        (&catalog, &tables, &mut runtime),
        0.0,
    );
    let handle = beams.slots()[0].handle;
    keep_at(
        &mut beams,
        &presented,
        (&catalog, &tables, &mut runtime),
        0.04,
    );
    assert_eq!(beams.slots().len(), 1);
    // The target died: its bubble is no longer presented, although the deadline is ahead.
    keep_at(
        &mut beams,
        std::slice::from_ref(&attacker),
        (&catalog, &tables, &mut runtime),
        0.08,
    );
    assert!(beams.slots().is_empty(), "released at once");
    let instance = runtime.held(handle).expect("eroding out");
    assert_eq!(instance.released_at, Some(0.08));
    let exit = catalog.effect(instance.effect).expect("loaded").held_exit;
    assert!(exit <= client::effects::catalog::MAX_SUSTAIN_EXIT);
    // Nothing of it is drawn once its exit is over, its onset's pulses included: the next
    // frame's keep ends it.
    let after = 0.08 + f64::from(exit) + 0.01;
    keep_at(
        &mut beams,
        &[attacker],
        (&catalog, &tables, &mut runtime),
        after,
    );
    assert!(runtime.held(handle).is_none(), "ended after its exit");
    runtime.build_frame(&catalog, after, None);
    assert!(!draws(&runtime, &catalog), "no beam pixel after its exit");
}

#[test]
fn the_beam_ends_a_period_and_grace_after_the_last_strike() {
    let (catalog, units, tables) = shipped();
    let (source, effect) = beam_unit(&catalog, &units);
    let grace = f64::from(
        catalog
            .effect(effect)
            .and_then(|look| look.sustain)
            .expect("sustained")
            .grace,
    );
    let period = f64::from(BEAM_PERIOD_TICKS) / f64::from(sim_types::TICKS_PER_SECOND);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let presented = [attacker.clone(), target.clone()];
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let last = 1.0;
    strike_at(
        &mut beams,
        &beam_strike(source, &attacker, &target),
        &presented,
        (&catalog, &tables, &mut runtime),
        last,
    );
    let deadline = beams.slots()[0].deadline;
    assert!(
        (deadline - (last + period + grace)).abs() < 1e-6,
        "{deadline}"
    );
    keep_at(
        &mut beams,
        &presented,
        (&catalog, &tables, &mut runtime),
        deadline - 0.001,
    );
    assert_eq!(beams.slots().len(), 1, "held until the deadline");
    keep_at(
        &mut beams,
        &presented,
        (&catalog, &tables, &mut runtime),
        deadline + 0.001,
    );
    assert!(beams.slots().is_empty(), "released once strikes stopped");
}

#[test]
fn a_full_table_refuses_and_counts() {
    let (catalog, units, tables) = shipped();
    let (source, _) = beam_unit(&catalog, &units);
    let target = beam_bubble(1000, source.unit_type, 30.0, 30.0);
    let attackers: Vec<BubbleRenderInstance> = (0..=HELD_BEAMS)
        .map(|index| {
            let id = u32::try_from(index).expect("small") + 1;
            #[allow(clippy::cast_precision_loss)]
            let at = 2.0 + index as f32 * 0.4;
            beam_bubble(id, source.unit_type, at, 4.0)
        })
        .collect();
    let mut presented = attackers.clone();
    presented.push(target.clone());
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    for attacker in &attackers {
        assert!(strike_at(
            &mut beams,
            &beam_strike(source, attacker, &target),
            &presented,
            (&catalog, &tables, &mut runtime),
            0.0,
        ));
    }
    assert_eq!(beams.slots().len(), HELD_BEAMS);
    assert_eq!(
        beams.refused, 1,
        "the beam past the table's capacity is refused"
    );
    assert!(
        beams
            .buffers()
            .iter()
            .all(|(_, _, capacity)| *capacity == HELD_BEAMS),
        "the table never grows"
    );
}

/// A context at effect time `now` and battle time `battle_now`.
fn context_at<'a>(
    (catalog, tables, runtime): (
        &'a EffectCatalog,
        &'a UnitEffectTables,
        &'a mut EffectRuntime,
    ),
    now: f64,
    battle_now: f64,
) -> BeamContext<'a> {
    BeamContext {
        catalog,
        tables,
        runtime,
        now,
        battle_now,
    }
}

/// Review round 1, R2-3: a paused battle (speed 0) stops the battle clock while the effects'
/// clock runs on. The channelling beam is neither released nor eroded while paused, and on
/// resume it is the same beam (no second hold, no replayed onset); its deadline counts battle
/// time only.
#[test]
fn a_paused_battle_freezes_the_beam_and_resumes_it_without_a_new_onset() {
    let (catalog, units, tables) = shipped();
    let (source, effect) = beam_unit(&catalog, &units);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let presented = [attacker.clone(), target.clone()];
    let event = beam_strike(source, &attacker, &target);
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let line = travel_request(&event, &tables, &catalog).and_then(|shot| shot.line);
    let mut context = context_at((&catalog, &tables, &mut runtime), 1.0, 1.0);
    assert!(beams.strike(&event, line, &presented, &mut context));
    let handle = beams.slots()[0].handle;
    // Paused for ten seconds of effect time: the battle clock stays at 1.0.
    for frame in 1..=250_u32 {
        let now = 1.0 + f64::from(frame) * 0.04;
        let mut context = context_at((&catalog, &tables, &mut runtime), now, 1.0);
        beams.keep(&presented, None, None, &mut context);
    }
    assert_eq!(beams.slots().len(), 1, "still held after a long pause");
    assert!(
        runtime
            .held(handle)
            .is_some_and(|instance| instance.released_at.is_none()),
        "not eroding while paused"
    );
    // Resumed: the next strike refreshes the same beam.
    let mut context = context_at((&catalog, &tables, &mut runtime), 11.04, 1.04);
    beams.keep(&presented, None, None, &mut context);
    assert!(beams.strike(&event, line, &presented, &mut context));
    assert_eq!(beams.slots()[0].handle, handle, "the same beam");
    assert_eq!(runtime.spawned_of(effect), 1, "no second onset");
    // With no further strike it goes a period and the grace of battle time later.
    let deadline = beams.slots()[0].deadline;
    let mut context = context_at((&catalog, &tables, &mut runtime), 30.0, deadline + 0.01);
    beams.keep(&presented, None, None, &mut context);
    assert_eq!(beams.slots().len(), 0);
}

/// Review round 1, R2-7: a strike without a travel line holds nothing (no beam at the world
/// origin); it is still taken, so no shot is spawned for it, and the next strike with a line
/// holds the beam.
#[test]
fn a_strike_without_a_line_holds_no_beam_at_the_origin() {
    let (catalog, units, tables) = shipped();
    let (source, effect) = beam_unit(&catalog, &units);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let presented = [attacker.clone(), target.clone()];
    let event = beam_strike(source, &attacker, &target);
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let mut context = context_at((&catalog, &tables, &mut runtime), 1.0, 1.0);
    assert!(
        beams.strike(&event, None, &presented, &mut context),
        "taken"
    );
    assert_eq!(beams.slots().len(), 0);
    assert_eq!(runtime.spawned_of(effect), 0, "nothing held");
    assert_eq!(beams.refused, 0, "not a refusal");
}

/// Review round 1, R2-2: a strike whose look the effect pool cannot hold is refused and
/// counted, as one finding the table full is.
#[test]
fn a_full_effect_pool_refuses_and_counts() {
    let (mut catalog, units, tables) = shipped();
    let (source, _) = beam_unit(&catalog, &units);
    catalog.budget.effects = 1;
    let target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let first = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let second = beam_bubble(2, source.unit_type, 4.0, 6.0);
    let presented = [first.clone(), second.clone(), target.clone()];
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    for attacker in [&first, &second] {
        let event = beam_strike(source, attacker, &target);
        assert!(strike_at(
            &mut beams,
            &event,
            &presented,
            (&catalog, &tables, &mut runtime),
            1.0
        ));
    }
    assert_eq!(beams.slots().len(), 1, "the pool holds one effect");
    assert_eq!(beams.refused, 1, "the second is refused and counted");
}

/// Review round 1, R2-2: a battle's table is sized from the catalog and the effect budget:
/// at most a quarter of the effect pool, and no more than the most beams one beam unit type
/// could hold if every player fielded only it.
#[test]
fn the_table_is_sized_from_the_catalog_and_the_budget() {
    let (mut catalog, units, _) = shipped();
    // The catalog bound, computed here on its own: per unit type, eight players fielding only
    // it at the supply cap, times its sustained weapons; the largest over the catalog. A
    // hangar's member costs no supply and is fielded only through its hangar units (plan
    // `2026-11-28-carrier-interceptors` D-11): the cap over the hangar unit's supply, times
    // its magazine.
    let cap = sim_core::production::STANDARD_SUPPLY_CAP;
    let per_player = |archetype: &UnitArchetype| {
        units
            .archetypes()
            .filter_map(|host| {
                host.capabilities
                    .hangar
                    .filter(|hangar| hangar.member == archetype.unit_type_id)
                    .map(|hangar| cap / host.supply_cost.max(1) * u32::from(hangar.max_count))
            })
            .max()
            .unwrap_or(cap / archetype.supply_cost.max(1))
    };
    let sustained = |catalog: &EffectCatalog, name: &Option<String>| {
        name.as_ref()
            .and_then(|name| catalog.effect_id(name))
            .and_then(|id| catalog.effect(id))
            .is_some_and(|effect| effect.sustain.is_some())
    };
    let expected = units
        .archetypes()
        .map(|archetype| {
            let weapons = archetype
                .effects
                .weapons
                .iter()
                .filter(|entry| sustained(&catalog, &entry.travel))
                .count();
            let per_player = usize::try_from(per_player(archetype)).expect("small");
            sim_types::MAX_PLAYERS * per_player * weapons
        })
        .max()
        .expect("the shipped catalog has units");
    // The shipped data: the Saucer Interceptor, 8 per 6-supply Carrier, holds a beam on each of
    // its two weapons (above the cheapest supply-bounded beam units: 2 supply, one beam, 800).
    assert_eq!(
        expected, 4224,
        "8 players x 200 supply / 6 x 8 members x 2 beams"
    );
    let share = usize::try_from(catalog.budget.effects / 4).expect("small");
    let capacity = HeldBeams::capacity_for(&units, &catalog);
    eprintln!("held-beam table: {capacity} (pool share {share}, catalog bound {expected})");
    assert_eq!(capacity, share.min(expected), "the smaller of the two");
    let table = HeldBeams::with_capacity(capacity);
    assert_eq!(table.capacity(), capacity);
    assert!(table
        .buffers()
        .iter()
        .all(|(_, _, reserved)| *reserved >= capacity));
    // A pool large enough: the catalog bound decides.
    catalog.budget.effects = 1_000_000;
    assert_eq!(HeldBeams::capacity_for(&units, &catalog), expected);
    // No sustained look in the vocabulary: no beam is ever held, and the table is empty.
    for effect in &mut catalog.effects {
        effect.sustain = None;
    }
    assert_eq!(HeldBeams::capacity_for(&units, &catalog), 0);
    assert_eq!(HeldBeams::with_capacity(0).capacity(), 0);
}

/// Review round 2, R3-1: the burn a held beam's strike lays follows the beam's end while the
/// target moves, as do the previous strike's; the burn of a strike of no held beam stays.
#[test]
fn a_held_beams_burns_follow_its_end() {
    let (catalog, units, tables) = shipped();
    let (source, _) = beam_unit(&catalog, &units);
    let attacker = beam_bubble(1, source.unit_type, 4.0, 4.0);
    let mut target = beam_bubble(100, source.unit_type, 9.0, 4.0);
    let mut runtime = EffectRuntime::new(&catalog);
    let mut beams = HeldBeams::default();
    let mut burns = Vec::new();
    for strike in 0..2_u32 {
        let now = 1.0 + f64::from(strike) * 0.36;
        let presented = [attacker.clone(), target.clone()];
        let event = beam_strike(source, &attacker, &target);
        assert!(strike_at(
            &mut beams,
            &event,
            &presented,
            (&catalog, &tables, &mut runtime),
            now
        ));
        let impact = spawn_request(&event, &tables, &catalog).expect("the beam names a burn");
        let burn = runtime
            .spawn(&catalog, &impact, now)
            .expect("the burn spawns");
        beams.attach_burn(&event, burn);
        burns.push(burn);
        keep_at(
            &mut beams,
            &presented,
            (&catalog, &tables, &mut runtime),
            now,
        );
    }
    assert_eq!(beams.slots()[0].burns, [Some(burns[1]), Some(burns[0])]);
    // The target walks on: both burns stay on the beam's end.
    for step in 1..=4_u8 {
        target.world_pos = [
            9.0 + f32::from(step) * 0.3,
            4.0 + f32::from(step) * 0.2,
            0.0,
        ];
        let presented = [attacker.clone(), target.clone()];
        let now = 1.4 + f64::from(step) * 0.02;
        keep_at(
            &mut beams,
            &presented,
            (&catalog, &tables, &mut runtime),
            now,
        );
        let end = held_line(&runtime, beams.slots()[0].handle).to;
        for burn in &burns {
            let anchor = runtime.pool().get(*burn).expect("the burn lives").anchor;
            assert!(
                anchor.distance(end) < 1e-4,
                "step {step}: {anchor:?} vs {end:?}"
            );
        }
    }
    // A burn of a strike no beam holds is not attached.
    let mut stray = HeldBeams::default();
    let event = beam_strike(source, &attacker, &target);
    let impact = spawn_request(&event, &tables, &catalog).expect("the beam names a burn");
    let burn = runtime
        .spawn(&catalog, &impact, 2.0)
        .expect("the burn spawns");
    stray.attach_burn(&event, burn);
    assert_eq!(stray.slots().len(), 0);
}

/// W-20 (plan `2026-11-27-weapon-delivery` D-8): the shipped weapons whose missiles fly movers
/// (found by their movers, never by name) publish a projectile event that asks for their unit's
/// travel along the flight, a travel that flies (a layer lasting the flight), and for their
/// impact delayed by the flight, so the strike lands where and when the missile does; a look
/// without an impact draws its landing with its splash (plan `2026-11-28-missile-movers-b52`).
#[test]
fn a_mover_weapons_projectile_flies_its_travel_and_lands_its_impact_after_the_flight() {
    let (catalog, units, tables) = shipped();
    let mut checked = 0;
    for archetype in units.archetypes() {
        let structure = matches!(archetype.shape, sim_types::ShapeKind::BuildingDome { .. });
        let slots = [
            (
                if structure || archetype.ground_weapon != archetype.weapon {
                    WeaponSlot::Ground
                } else {
                    WeaponSlot::Primary
                },
                archetype
                    .ground_weapon
                    .as_ref()
                    .or(archetype.weapon.as_ref()),
            ),
            (WeaponSlot::Air, archetype.air_weapon.as_ref()),
        ];
        for (slot, weapon) in slots {
            if weapon.is_none_or(|weapon| weapon.missile_movers.is_empty()) {
                continue;
            }
            let source = EffectSource {
                unit_type: archetype.unit_type_id,
                weapon: WeaponKey { slot, mode: None },
            };
            let flight = 0.28;
            let event = CombatEventVisual::ProjectileVisual {
                pos: [4.0, 4.0, 0.5],
                vel: [10.0, 0.0, 0.0],
                owner: PlayerId(0),
                duration_secs: Some(flight),
                source,
                ends: None,
            };
            let travel = travel_request(&event, &tables, &catalog)
                .unwrap_or_else(|| panic!("{} names a travel", archetype.slug));
            assert_eq!(
                Some(travel.effect),
                tables.travel(source),
                "{}",
                archetype.slug
            );
            assert_eq!(travel.bound.get(Binding::FlightTime), flight);
            assert!(
                catalog.effect(travel.effect).is_some_and(flies),
                "{}'s travel flies the missile's path",
                archetype.slug
            );
            // A look without an impact draws the landing with its splash, whose event the
            // simulation publishes when the missile lands (the Liberator analogue's, plan
            // `2026-11-28-missile-movers-b52` WP4): the projectile event then spawns no impact.
            if tables.impact(source).is_none() {
                assert!(
                    tables.splash(source).is_some(),
                    "{}: a mover weapon's landing is drawn",
                    archetype.slug
                );
                assert!(
                    spawn_request(&event, &tables, &catalog).is_none(),
                    "{}",
                    archetype.slug
                );
                checked += 1;
                continue;
            }
            let impact = spawn_request(&event, &tables, &catalog)
                .unwrap_or_else(|| panic!("{} names an impact", archetype.slug));
            assert_eq!(
                Some(impact.effect),
                tables.impact(source),
                "{}",
                archetype.slug
            );
            assert_eq!(
                impact.delay, flight,
                "{}: the impact waits for the flight",
                archetype.slug
            );
            assert!(
                (impact.anchor - Vec3::new(4.0 + 10.0 * flight, 0.5, 4.0)).length() < 1e-4,
                "{}: the impact lands where the missile arrives",
                archetype.slug
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 2,
        "premise: an air and a structure weapon fly movers, {checked} found"
    );
}
