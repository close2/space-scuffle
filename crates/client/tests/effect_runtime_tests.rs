//! The effect runtime through its public API: events become spawns, spawns become this
//! frame's cards, decals and lights, under the budget and level-of-detail rules (plan
//! `2026-11-09-effect-visualization` §5.4, §5.6, D-7, D-12).
//!
//! # Responsibility
//! Drives [`EffectRuntime`] with a test-only vocabulary over the fixture texture set (written
//! into a temporary directory) and checks the bound sizes, ring radii, placements, the drop
//! and retirement order, level of detail, the back-to-front sort, determinism and that a
//! steady splash stream reuses every retained buffer.
//!
//! # Invariants verified
//! - Critical layers always draw; detail of the oldest `Minor` effect retires first.
//! - Every size follows its binding; a line effect spawns once per line.
//! - Two runtimes fed the same stream build identical frames.

#![allow(clippy::expect_used, clippy::panic, clippy::float_cmp)]

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

use bevy::math::Vec3;
use client::effects::catalog::{EffectCatalog, UnitEffectTables};
use client::effects::instances::{ORIENT_GROUND, ORIENT_VERTICAL};
use client::effects::runtime::{CameraView, EffectRuntime};
use client::effects::systems::spawn_request;
use client::ResolvedAssetsRoot;
use effect_fixtures::{vocabulary, write_fixture_set};
use sim_core::catalog::{ArchetypeEffects, WeaponEffects};
use sim_types::snapshot::CombatEventVisual;
use sim_types::{EffectSource, LineView, PlayerId, UnitTypeId, WeaponKey, WeaponSlot};

/// The effects of the test vocabulary.
const EFFECTS: &str = r#"
    "blast": (class: Major, event: Splash, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white"), size: (bind: SplashReach, scale: 2.0),
         duration: 1.0, priority: Critical),
        (block: Ring(texture: "solid", ramp: "green", orient: Ground, thickness: 0.25),
         radius: (from: (bind: SplashFull, scale: 1.0), to: (bind: SplashReach, scale: 1.0)),
         duration: 1.0, priority: Critical),
        (block: Decal(texture: "solid", ramp: "blue"), size: (bind: SplashReach, scale: 1.0),
         duration: 5.0, priority: Detail),
        (block: Light(color: (1.0, 0.5, 0.2), intensity: 1000.0),
         size: (bind: SplashReach, scale: 3.0), duration: 1.0, curve: "fade", priority: Detail),
        (block: Particles(emitter: "sparks"), size: (bind: SplashReach, scale: 1.0),
         priority: Detail),
    ]),
    "puff": (class: Minor, event: Splash, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 0.5),
         duration: 2.0, priority: Detail),
    ]),
    "spines": (class: Major, event: Line, layers: [
        (block: Sprite(texture: "flip", ramp: "red_add", orient: VerticalAlongLine),
         size: (scale: 0.4), duration: 1.0, priority: Critical,
         placement: Line(spacing: (bind: LineSpacing, scale: 1.0), travel: 10.0)),
    ])
"#;

/// The emitters of the test vocabulary.
const EMITTERS: &str = r#""sparks": (spawn: Burst(count: (base: 10, per_unit_radius: 4.0, max: 30)),
    shape: Disc, speed: (2.0, 3.0), cone_deg: 40.0, gravity: 9.0, drag: 0.5,
    lifetime: (0.4, 0.8), size: (0.05, 0.1), texture: "soft_disc",
    orient: VelocityStretched(1.0), ramp: "red_add")"#;

/// Fixture: the vocabulary loaded from a temporary directory.
fn catalog_with(budget_edit: impl Fn(String) -> String) -> (tempfile::TempDir, EffectCatalog) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let manifest = write_fixture_set(dir.path());
    let text = budget_edit(vocabulary(&manifest, EMITTERS, EFFECTS, ""));
    let catalog = EffectCatalog::from_text(&text, &dir.path().join("effects.ron"), dir.path())
        .expect("the test vocabulary is valid");
    (dir, catalog)
}

/// The shipped units with the first archetype's primary weapon splashing `splash`.
fn tables_for(catalog: &EffectCatalog, splash: &str) -> (UnitEffectTables, UnitTypeId) {
    let mut units = sim_fixtures::shipped_catalogs().units;
    let id = units
        .archetypes()
        .next()
        .expect("an archetype")
        .unit_type_id;
    units.get_mut(id).expect("it exists").effects = ArchetypeEffects {
        weapons: vec![WeaponEffects {
            weapon: WeaponSlot::Primary,
            mode: None,
            launch: None,
            travel: None,
            impact: None,
            splash: Some(splash.to_owned()),
        }],
        ..ArchetypeEffects::default()
    };
    (
        UnitEffectTables::build(&units, catalog).expect("the block resolves"),
        id,
    )
}

/// A splash event of `unit` at map point `(x, y)` with zones `zones`.
fn splash(
    unit: UnitTypeId,
    x: f32,
    y: f32,
    zones: [f32; 3],
    line: Option<LineView>,
) -> CombatEventVisual {
    CombatEventVisual::SplashVisual {
        center: [x, y, 0.0],
        zones,
        line,
        source: EffectSource {
            unit_type: unit,
            weapon: WeaponKey::default(),
        },
        owner: PlayerId(0),
    }
}

/// A camera 20 units above and behind the origin, looking at it.
fn camera() -> CameraView {
    let position = Vec3::new(0.0, 20.0, 20.0);
    CameraView {
        position,
        forward: (-position).normalize(),
        fov_y: std::f32::consts::FRAC_PI_4,
        viewport_px: 720.0,
        aspect: 16.0 / 9.0,
    }
}

/// Spawns `event` at `now` through the production lookup.
fn spawn(
    runtime: &mut EffectRuntime,
    catalog: &EffectCatalog,
    tables: &UnitEffectTables,
    event: &CombatEventVisual,
    now: f64,
) -> bool {
    spawn_request(event, tables, catalog)
        .and_then(|request| runtime.spawn(catalog, &request, now))
        .is_some()
}

#[test]
fn a_splash_draws_every_block_sized_by_its_zones() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "blast");
    let mut runtime = EffectRuntime::new(&catalog);
    assert!(spawn(
        &mut runtime,
        &catalog,
        &tables,
        &splash(unit, 3.0, 4.0, [0.5, 0.8, 1.25], None),
        0.0
    ));
    runtime.build_frame(&catalog, 0.5, Some(&camera()));
    let frame = &runtime.frame;
    let sprite = frame
        .sorted
        .iter()
        .find(|card| card.ring_inner == 0.0 && card.half_size > 2.0)
        .expect("the blast sprite");
    assert!((sprite.half_size - 2.5).abs() < 1e-5, "2 x reach");
    assert_eq!(
        sprite.center,
        [3.0, 0.0, 4.0],
        "snapshot (x, y, alt) maps to world (x, alt, y)"
    );
    let ring = frame
        .sorted
        .iter()
        .find(|card| card.ring_inner > 0.0)
        .expect("the ring");
    assert!(
        (ring.half_size - (0.5 + (1.25 - 0.5) * 0.5)).abs() < 1e-5,
        "halfway from full to reach"
    );
    assert!((ring.ring_inner - 0.75).abs() < 1e-6);
    assert_eq!(ring.orient, ORIENT_GROUND);
    assert_eq!(frame.decals.len(), 1);
    assert_eq!(frame.decals[0].half_extents[0], 1.25);
    assert_eq!(frame.lights.len(), 1);
    assert!((frame.lights[0].range - 3.75).abs() < 1e-5);
    assert!(
        (frame.lights[0].intensity - 500.0).abs() < 1e-2,
        "fade curve at half life"
    );
    assert!(!runtime.particles.is_empty(), "the burst fired");
    let burst = runtime.particles.len();
    assert!(
        burst <= 15,
        "10 + 4 x 1.25 particles at most, scaled by size: {burst}"
    );

    runtime.build_frame(&catalog, 1.2, Some(&camera()));
    assert!(
        runtime
            .frame
            .sorted
            .iter()
            .all(|card| card.ring_inner == 0.0 && card.half_size < 1.0),
        "sprite and ring end at 1 s; only particles may remain"
    );
    assert_eq!(runtime.frame.decals.len(), 1, "the decal lingers");
    runtime.build_frame(&catalog, 5.5, Some(&camera()));
    assert!(runtime.frame.decals.is_empty() && runtime.frame.sorted.is_empty());
    assert_eq!(runtime.pool.live_total(), 0, "the effect ended");
}

#[test]
fn shipped_data_names_no_effect_so_nothing_is_spawned() {
    let root = ResolvedAssetsRoot::resolve().expect("assets root");
    let catalog = EffectCatalog::load(&root.as_path().join("catalog/effects.ron"), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = UnitEffectTables::build(&units, &catalog).expect("tables");
    for archetype in units.archetypes() {
        let event = splash(archetype.unit_type_id, 1.0, 1.0, [1.0, 1.0, 1.0], None);
        assert!(
            spawn_request(&event, &tables, &catalog).is_none(),
            "{}",
            archetype.slug
        );
    }
}

#[test]
fn a_line_effect_spawns_once_per_line_and_places_along_it() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "spines");
    let mut runtime = EffectRuntime::new(&catalog);
    let line = |index| LineView {
        from: [0.0, 0.0, 0.0],
        to: [9.0, 0.0, 0.0],
        spacing: 1.0,
        index,
        count: 9,
    };
    let mut spawned = 0;
    for index in 0..9 {
        let event = splash(
            unit,
            1.0 + f32::from(index),
            0.0,
            [0.3, 0.3, 0.3],
            Some(line(index)),
        );
        spawned += u32::from(spawn(&mut runtime, &catalog, &tables, &event, 0.0));
    }
    assert_eq!(
        spawned, 1,
        "only the first search point spawns the line's effect"
    );
    runtime.build_frame(&catalog, 0.95, Some(&camera()));
    let cards = &runtime.frame.sorted;
    assert_eq!(
        cards.len(),
        9,
        "points 1..=9: eight spacings from the first search point"
    );
    assert!(cards
        .iter()
        .all(|card| card.orient == ORIENT_VERTICAL && card.axis == [1.0, 0.0, 0.0]));
    let xs: Vec<f32> = cards.iter().map(|card| card.center[0]).collect();
    assert!(
        xs.iter().all(|x| (1.0..=9.0 + 1e-4).contains(x)),
        "never at the attacker: {xs:?}"
    );
    assert!(
        cards.iter().all(|card| (card.center[1] - 0.4).abs() < 1e-5),
        "standing on the ground"
    );
    runtime.build_frame(&catalog, 0.05, Some(&camera()));
    let early = runtime.frame.sorted.len();
    assert!(
        early < 9,
        "travel delays the far spines: {early} drawn at 0.05 s"
    );
}

#[test]
fn critical_layers_survive_a_full_pool_and_the_oldest_minor_detail_retires() {
    let (_dir, catalog) = catalog_with(|text| text.replace("sprites: 256", "sprites: 1"));
    let (blast_tables, unit) = tables_for(&catalog, "blast");
    let (puff_tables, _) = tables_for(&catalog, "puff");
    let mut runtime = EffectRuntime::new(&catalog);
    assert!(spawn(
        &mut runtime,
        &catalog,
        &puff_tables,
        &splash(unit, 0.0, 0.0, [1.0; 3], None),
        0.0
    ));
    assert!(spawn(
        &mut runtime,
        &catalog,
        &puff_tables,
        &splash(unit, 1.0, 0.0, [1.0; 3], None),
        0.1
    ));
    runtime.build_frame(&catalog, 0.2, None);
    assert_eq!(
        runtime.frame.sorted.len(),
        1,
        "one detail card fits the budget of one"
    );
    assert_eq!(runtime.stats.detail_skipped, 1);
    assert_eq!(
        runtime.stats.retired, 1,
        "the oldest Minor's detail retires"
    );

    assert!(spawn(
        &mut runtime,
        &catalog,
        &blast_tables,
        &splash(unit, 5.0, 5.0, [1.0; 3], None),
        0.2
    ));
    runtime.build_frame(&catalog, 0.25, None);
    let critical = runtime
        .frame
        .sorted
        .iter()
        .filter(|card| card.half_size >= 1.0)
        .count();
    assert_eq!(
        critical, 2,
        "both critical layers of the blast draw past the budget"
    );
    assert_eq!(runtime.stats.critical_overflow, 0);

    runtime.build_frame(&catalog, 0.36, None);
    let retired = runtime
        .frame
        .sorted
        .iter()
        .filter(|card| (card.half_size - 0.5).abs() < 1e-6)
        .count();
    assert!(
        retired <= 1,
        "retired detail is gone 0.1 s after retirement"
    );
}

#[test]
fn an_effect_over_its_class_cap_draws_only_its_critical_layers() {
    let (_dir, catalog) = catalog_with(|text| text.replace("Major: 16", "Major: 1"));
    let (tables, unit) = tables_for(&catalog, "blast");
    let mut runtime = EffectRuntime::new(&catalog);
    assert!(spawn(
        &mut runtime,
        &catalog,
        &tables,
        &splash(unit, 0.0, 0.0, [1.0; 3], None),
        0.0
    ));
    assert!(spawn(
        &mut runtime,
        &catalog,
        &tables,
        &splash(unit, 9.0, 0.0, [1.0; 3], None),
        0.0
    ));
    runtime.build_frame(&catalog, 0.2, None);
    assert_eq!(
        runtime.frame.decals.len(),
        1,
        "the second blast's detail decal is retired at birth"
    );
    let critical_cards = runtime
        .frame
        .sorted
        .iter()
        .filter(|card| card.half_size >= 1.0)
        .count();
    assert_eq!(critical_cards, 4, "both blasts draw their sprite and ring");
}

#[test]
fn layers_below_the_minimum_screen_size_are_skipped() {
    let (_dir, catalog) =
        catalog_with(|text| text.replace("min_screen_px: 0.0", "min_screen_px: 100.0"));
    let (tables, unit) = tables_for(&catalog, "blast");
    let mut runtime = EffectRuntime::new(&catalog);
    assert!(spawn(
        &mut runtime,
        &catalog,
        &tables,
        &splash(unit, 0.0, 0.0, [0.5, 0.8, 1.25], None),
        0.0
    ));
    runtime.build_frame(&catalog, 0.1, Some(&camera()));
    let cards = &runtime.frame.sorted;
    assert!(
        cards.iter().any(|card| (card.half_size - 2.5).abs() < 1e-5),
        "the 5-unit sprite (about 150 px) stays"
    );
    assert!(
        cards.iter().all(|card| card.ring_inner == 0.0),
        "the 1.15-unit ring (about 35 px) is skipped"
    );
    assert!(runtime.stats.lod_skipped > 0);
}

#[test]
fn cards_are_sorted_back_to_front() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "puff");
    let mut runtime = EffectRuntime::new(&catalog);
    for y in [-5.0, 8.0, 0.0, 3.0, -9.0] {
        assert!(spawn(
            &mut runtime,
            &catalog,
            &tables,
            &splash(unit, 0.0, y, [1.0; 3], None),
            0.0
        ));
    }
    let view = camera();
    runtime.build_frame(&catalog, 0.1, Some(&view));
    let depths: Vec<f32> = runtime
        .frame
        .sorted
        .iter()
        .map(|card| view.depth(Vec3::from_array(card.center)))
        .collect();
    assert_eq!(depths.len(), 5);
    assert!(
        depths.windows(2).all(|pair| pair[0] >= pair[1]),
        "far first: {depths:?}"
    );
}

#[test]
fn the_same_stream_builds_the_same_frames_and_reuses_every_buffer() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "blast");
    let mut a = EffectRuntime::new(&catalog);
    let mut b = EffectRuntime::new(&catalog);
    let view = camera();
    let mut buffers = None;
    for frame in 0..200_u16 {
        let now = f64::from(frame) * 0.04;
        if frame % 3 == 0 {
            let event = splash(
                unit,
                f32::from(frame % 11),
                f32::from(frame % 7),
                [0.4, 0.7, 1.0],
                None,
            );
            spawn(&mut a, &catalog, &tables, &event, now);
            spawn(&mut b, &catalog, &tables, &event, now);
        }
        a.build_frame(&catalog, now, Some(&view));
        b.build_frame(&catalog, now, Some(&view));
        assert_eq!(a.frame, b.frame, "frame {frame}");
        if frame == 50 {
            buffers = Some(a.frame.buffers());
        }
    }
    assert_eq!(
        Some(a.frame.buffers()),
        buffers,
        "no retained buffer moved or grew"
    );
    assert!(a.stats.spawned > 60);
}
