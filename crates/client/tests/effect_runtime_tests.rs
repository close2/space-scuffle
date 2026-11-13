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
//! - A class cap counts an effect only while its fire is alive, and over it the oldest burning
//!   effect gives its detail up, never the newborn; at the decal cap marks draw whole and never
//!   flicker (plan Amendment 7).
//! - Every size follows its binding; a line effect spawns once per line.
//! - Two runtimes fed the same stream build identical frames.

#![allow(clippy::expect_used, clippy::panic, clippy::float_cmp)]

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

use bevy::math::Vec3;
use client::battle_pipeline::commands::SessionGeneration;
use client::battle_pipeline::fog::{BattleFog, Presence};
use client::effects::catalog::error::EffectCatalogError;
use client::effects::catalog::{EffectCatalog, UnitEffectTables};
use client::effects::instances::{FLAG_DARKEN, ORIENT_GROUND, ORIENT_VERTICAL};
use client::effects::runtime::{CameraView, EffectRuntime};
use client::effects::systems::trail_request;
use client::effects::systems::{spawn_request, LineAssembly};
use client::ResolvedAssetsRoot;
use effect_fixtures::{vocabulary, write_fixture_set};
use sim_core::catalog::{ArchetypeEffects, ModeEffects, WeaponEffects};
use sim_types::snapshot::CombatEventVisual;
use sim_types::{
    BubbleRenderInstance, Concealment, EffectSource, LineView, PlayerId, PlayerView, UnitTypeId,
    WeaponKey, WeaponSlot,
};

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
    ]),
    "trail": (class: Minor, event: Mode, repeat: Some(0.2), layers: [
        (block: Decal(texture: "solid", ramp: "blue"), size: (bind: UnitRadius, scale: 1.0),
         duration: 1.0, priority: Detail),
    ]),
    "mark": (class: Minor, event: Splash, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 0.5),
         duration: 0.3, priority: Critical),
        (block: Decal(texture: "solid", ramp: "blue"), size: (scale: 0.5),
         placement: Disc(count: 2, radius: (scale: 0.5)), duration: 3.0, priority: Critical),
    ]),
    "furrow": (class: Minor, event: Splash, layers: [
        (block: Ring(texture: "solid", ramp: "green", orient: Ground, thickness: 0.25, darken: true),
         radius: (from: (scale: 1.0), to: (scale: 1.0)), duration: 1.0, priority: Critical),
    ]),
    "trail_once": (class: Minor, event: Mode, layers: [
        (block: Decal(texture: "solid", ramp: "blue"), size: (bind: UnitRadius, scale: 1.0),
         duration: 1.0, priority: Detail),
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

/// The shipped units with the first archetype's primary weapon splashing `splash` and no other
/// presentation block.
fn tables_for(catalog: &EffectCatalog, splash: &str) -> (UnitEffectTables, UnitTypeId) {
    let mut units = sim_fixtures::shipped_catalogs().units;
    let ids: Vec<UnitTypeId> = units
        .archetypes()
        .map(|archetype| archetype.unit_type_id)
        .collect();
    let id = *ids.first().expect("an archetype");
    // The shipped blocks name effects the test vocabulary does not define.
    for other in &ids {
        units.get_mut(*other).expect("it exists").effects = ArchetypeEffects::default();
    }
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
fn shipped_data_spawns_only_the_splash_effects_units_name() {
    let root = ResolvedAssetsRoot::resolve().expect("assets root");
    let catalog = EffectCatalog::load(&root.as_path().join("catalog/effects.ron"), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = UnitEffectTables::build(&units, &catalog).expect("tables");
    for archetype in units.archetypes() {
        let event = splash(archetype.unit_type_id, 1.0, 1.0, [1.0, 1.0, 1.0], None);
        // The event's source is the primary weapon in the default stance; there are no
        // defaults, so only a block naming that weapon's splash spawns anything.
        let named = archetype.effects.weapons.iter().any(|entry| {
            entry.weapon == WeaponSlot::Primary && entry.mode.is_none() && entry.splash.is_some()
        });
        assert_eq!(
            spawn_request(&event, &tables, &catalog).is_some(),
            named,
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
    let mut lines = LineAssembly::default();
    for index in 0..9 {
        let event = splash(
            unit,
            1.0 + f32::from(index),
            0.0,
            [0.3, 0.3, 0.3],
            Some(line(index)),
        );
        let request = spawn_request(&event, &tables, &catalog).expect("the spines are named");
        if let Some(ready) = lines.push(request) {
            spawned += u32::from(runtime.spawn(&catalog, &ready, 0.0).is_some());
        }
    }
    let whole = lines.finish().expect("the line is still being merged");
    assert_eq!(
        whole.line.map(|line| line.seen & 0x1FF),
        Some(0x1FF),
        "every point seen"
    );
    spawned += u32::from(runtime.spawn(&catalog, &whole, 0.0).is_some());
    assert_eq!(spawned, 1, "the line's points spawn its effect once");
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

/// Plan Amendment 2, R6: the fog admits a line point by point. A line whose first three
/// points are fogged (only points 3 to 8 arrive) still spawns, anchored at its true first
/// point, and draws only at the points that arrived.
#[test]
fn a_line_whose_first_points_are_fogged_shows_where_it_is_seen() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "spines");
    let mut runtime = EffectRuntime::new(&catalog);
    let mut lines = LineAssembly::default();
    let mut ready = Vec::new();
    for index in 3..9_u8 {
        let event = splash(
            unit,
            1.0 + f32::from(index),
            0.0,
            [0.3, 0.3, 0.3],
            Some(LineView {
                from: [0.0, 0.0, 0.0],
                to: [9.0, 0.0, 0.0],
                spacing: 1.0,
                index,
                count: 9,
            }),
        );
        let request = spawn_request(&event, &tables, &catalog).expect("the spines are named");
        ready.extend(lines.push(request));
    }
    ready.extend(lines.finish());
    assert_eq!(ready.len(), 1, "one spawn for the line");
    let request = ready[0];
    assert!(
        (request.anchor - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-4,
        "anchored at the first search point: {:?}",
        request.anchor
    );
    let line = request.line.expect("a line");
    assert_eq!(line.seen & 0x1FF, 0x1F8, "points 3 to 8 seen");
    assert!(runtime.spawn(&catalog, &request, 0.0).is_some());
    runtime.build_frame(&catalog, 0.95, Some(&camera()));
    let xs: Vec<f32> = runtime
        .frame
        .sorted
        .iter()
        .map(|card| card.center[0])
        .collect();
    assert_eq!(xs.len(), 6, "six seen points: {xs:?}");
    assert!(
        xs.iter().all(|x| *x >= 4.0 - 1e-4),
        "nothing at the fogged points 1 to 3: {xs:?}"
    );
}

/// Two lines in one frame stay two spawns, and a splash between them passes straight through.
#[test]
fn the_line_assembly_keeps_lines_apart() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "spines");
    let point = |to: f32, index: u8| {
        let event = splash(
            unit,
            1.0 + f32::from(index),
            0.0,
            [0.3, 0.3, 0.3],
            Some(LineView {
                from: [0.0, 0.0, 0.0],
                to: [to, 0.0, 0.0],
                spacing: 1.0,
                index,
                count: 3,
            }),
        );
        spawn_request(&event, &tables, &catalog).expect("the spines are named")
    };
    let mut lines = LineAssembly::default();
    assert!(lines.push(point(3.0, 0)).is_none());
    assert!(lines.push(point(3.0, 1)).is_none());
    let first = lines
        .push(point(5.0, 0))
        .expect("a new line flushes the first");
    assert_eq!(first.line.map(|line| line.seen), Some(0b11));
    let second = lines.finish().expect("the second line");
    assert_eq!(second.line.map(|line| line.seen), Some(0b1));
    assert!(lines.finish().is_none());
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
fn over_its_class_cap_the_oldest_burning_effect_gives_its_detail_up() {
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
        "one blast's detail decal is retired"
    );
    assert!(
        runtime.frame.decals[0].center[0] > 4.0,
        "the newborn keeps its detail; the older blast gives it up"
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
fn an_effect_whose_fire_has_ended_no_longer_counts_against_its_class_cap() {
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
    // The first blast's cards, light and sparks are over by 1 s; its 5 s mark lasts.
    runtime.build_frame(&catalog, 2.0, None);
    assert!(spawn(
        &mut runtime,
        &catalog,
        &tables,
        &splash(unit, 9.0, 0.0, [1.0; 3], None),
        2.0
    ));
    runtime.build_frame(&catalog, 2.1, None);
    assert_eq!(runtime.stats.retired, 0, "nothing gives its detail up");
    assert_eq!(
        runtime.frame.decals.len(),
        2,
        "the lasting mark of the first and the detail of the second both draw"
    );
}

/// A decal pool of 8 under a stream of effects of two lasting marks each, one every 0.1 s
/// for 2.5 s (Amendment 7): a decal drawn in one frame is drawn in the next unless its effect
/// is eroding it out or its layer has ended, every effect draws its marks whole or not at all,
/// and the pool is never overrun.
#[test]
fn lasting_marks_at_the_decal_cap_never_flicker_and_draw_whole() {
    let (_dir, catalog) = catalog_with(|text| text.replace("decals: 64", "decals: 8"));
    let (tables, unit) = tables_for(&catalog, "mark");
    let mut runtime = EffectRuntime::new(&catalog);
    let mut previous: Vec<(u32, u64)> = Vec::new();
    let mut spawned = 0_u16;
    for step in 0..100_u32 {
        let now = f64::from(step) * 0.04;
        while f64::from(spawned) * 0.1 <= now && spawned < 25 {
            let x = f32::from(spawned % 5) * 3.0;
            let y = f32::from(spawned / 5) * 3.0;
            assert!(spawn(
                &mut runtime,
                &catalog,
                &tables,
                &splash(unit, x, y, [1.0; 3], None),
                now
            ));
            spawned += 1;
        }
        runtime.build_frame(&catalog, now, None);
        assert!(runtime.frame.decals.len() <= 8, "the pool is never overrun");
        // Owners this frame, identified by slot and birth, with their decal counts.
        let mut owners: Vec<(u32, u64, usize)> = Vec::new();
        for slot in &runtime.frame.decal_owners {
            let birth = runtime
                .pool
                .iter()
                .find(|(index, _)| *index == *slot as usize)
                .map(|(_, instance)| instance.birth.to_bits())
                .expect("a drawn decal belongs to a live effect");
            match owners
                .iter_mut()
                .find(|(s, b, _)| *s == *slot && *b == birth)
            {
                Some(owner) => owner.2 += 1,
                None => owners.push((*slot, birth, 1)),
            }
        }
        for (slot, _, count) in &owners {
            assert_eq!(
                *count, 2,
                "effect in slot {slot} draws its marks whole at {now:.2} s"
            );
        }
        for (slot, birth) in &previous {
            let still = owners.iter().any(|(s, b, _)| s == slot && b == birth);
            if still {
                continue;
            }
            let instance = runtime
                .pool
                .iter()
                .find(|(index, instance)| {
                    *index == *slot as usize && instance.birth.to_bits() == *birth
                })
                .map(|(_, instance)| *instance);
            if let Some(instance) = instance {
                assert!(
                    instance.marks_retired_at.is_some() || now > instance.decal_end,
                    "the marks of slot {slot} vanished at {now:.2} s without eroding out"
                );
            }
        }
        previous = owners.iter().map(|(s, b, _)| (*s, *b)).collect();
    }
    assert!(
        runtime.stats.marks_retired > 0,
        "the oldest marks made room"
    );
    assert_eq!(runtime.stats.critical_overflow, 0);
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

/// WP8 review 3, E3-1: darkening cards sort as the farthest of all, so they are the first
/// `darkening_cards` cards of the sorted buffer, which the renderer draws in one darkening item
/// before every other transparent item; the other cards stay back to front after them.
#[test]
fn darkening_cards_lead_the_sorted_cards() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (puffs, puff_unit) = tables_for(&catalog, "puff");
    let (furrows, furrow_unit) = tables_for(&catalog, "furrow");
    let mut runtime = EffectRuntime::new(&catalog);
    for (index, y) in [-5.0, 8.0, 0.0, 3.0, -9.0, 6.0].into_iter().enumerate() {
        let (tables, unit) = if index % 2 == 0 {
            (&puffs, puff_unit)
        } else {
            (&furrows, furrow_unit)
        };
        assert!(spawn(
            &mut runtime,
            &catalog,
            tables,
            &splash(unit, 0.0, y, [1.0; 3], None),
            0.0
        ));
    }
    let view = camera();
    runtime.build_frame(&catalog, 0.1, Some(&view));
    let frame = &runtime.frame;
    let darkening = frame.darkening_cards as usize;
    assert_eq!(darkening, 3, "one darkening ring per furrow");
    assert_eq!(frame.sorted.len(), 6);
    assert!(
        frame.sorted[..darkening]
            .iter()
            .all(|card| card.flags & FLAG_DARKEN != 0),
        "the darkening cards come first"
    );
    assert!(
        frame.sorted[darkening..]
            .iter()
            .all(|card| card.flags & FLAG_DARKEN == 0),
        "and only they"
    );
    let depths: Vec<f32> = frame.sorted[darkening..]
        .iter()
        .map(|card| view.depth(Vec3::from_array(card.center)))
        .collect();
    assert!(
        depths.windows(2).all(|pair| pair[0] >= pair[1]),
        "the others far first: {depths:?}"
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

/// A vocabulary whose splash throws particles that land with a `Point` effect, and a line
/// effect whose cards and particles are clipped to the line.
const LANDING_EFFECTS: &str = r#"
    "throw": (class: Major, event: Splash, layers: [
        (block: Particles(emitter: "globs"), size: (scale: 0.2)),
    ]),
    "fleck": (class: Minor, event: Point, layers: [
        (block: Decal(texture: "solid", ramp: "blue"), size: (scale: 0.2), duration: 2.0),
    ]),
    "clipped": (class: Major, event: Line, layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white", orient: Upright, lean_deg: 20.0,
            clip_to_line: true),
         size: (scale: 0.4), duration: 1.0, priority: Critical,
         placement: Line(spacing: (bind: LineSpacing, scale: 1.0), per_point: 2, scatter: 0.3)),
        (block: Particles(emitter: "chips", clip_to_line: true),
         placement: Line(spacing: (bind: LineSpacing, scale: 1.0))),
    ])
"#;

/// The emitters of [`LANDING_EFFECTS`].
const LANDING_EMITTERS: &str = r#"
    "globs": (spawn: Burst(count: (base: 6, max: 6)), shape: Disc, speed: (2.0, 3.0),
        cone_deg: 30.0, gravity: 12.0, lifetime: (2.0, 2.0), size: (0.05, 0.1),
        texture: "soft_disc", orient: Camera, ramp: "green", land: "fleck"),
    "chips": (spawn: Burst(count: (base: 3, max: 3)), shape: Point, speed: (1.0, 2.0),
        cone_deg: 60.0, gravity: 9.0, lifetime: (0.5, 0.5), size: (0.05, 0.05),
        texture: "soft_disc", orient: Camera, ramp: "red_add")
"#;

/// The landing vocabulary loaded from a temporary directory.
fn landing_catalog() -> (tempfile::TempDir, EffectCatalog) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let manifest = write_fixture_set(dir.path());
    let text = vocabulary(&manifest, LANDING_EMITTERS, LANDING_EFFECTS, "");
    let catalog = EffectCatalog::from_text(&text, &dir.path().join("effects.ron"), dir.path())
        .expect("the landing vocabulary is valid");
    (dir, catalog)
}

#[test]
fn thrown_particles_leave_their_landing_effect_where_they_land() {
    use client::effects::binding::BoundValues;
    use client::effects::runtime::SpawnRequest;
    let (_dir, catalog) = landing_catalog();
    let mut runtime = EffectRuntime::new(&catalog);
    let request = SpawnRequest {
        effect: catalog.effect_id("throw").expect("throw"),
        anchor: Vec3::new(4.0, 0.0, 4.0),
        bound: BoundValues::default(),
        line: None,
        delay: 0.0,
    };
    runtime.spawn(&catalog, &request, 0.0);
    let buffers = runtime.frame.buffers();
    for step in 0..30 {
        runtime.build_frame(&catalog, f64::from(step) * 0.04, None);
    }
    assert_eq!(
        runtime.stats.spawned,
        1 + 6,
        "every glob landed and spawned one fleck"
    );
    assert!(
        runtime.particles.is_empty(),
        "the globs ended where they landed"
    );
    assert_eq!(runtime.frame.decals.len(), 6, "six flecks on the ground");
    for decal in &runtime.frame.decals {
        assert!(
            decal.center[1].abs() < 1e-5,
            "at the height the globs started at"
        );
        let away = Vec3::from_array(decal.center) - request.anchor;
        assert!(
            away.length() > 0.05 && away.length() < 3.0,
            "thrown away from the anchor"
        );
    }
    assert_eq!(runtime.frame.buffers(), buffers, "no retained buffer grew");
}

#[test]
fn line_clipped_cards_and_particles_carry_the_line_window() {
    use client::effects::binding::BoundValues;
    use client::effects::catalog::schema::Binding;
    use client::effects::placement::WorldLine;
    use client::effects::runtime::SpawnRequest;
    let (_dir, catalog) = landing_catalog();
    let mut runtime = EffectRuntime::new(&catalog);
    let line = WorldLine::whole(Vec3::new(1.0, 0.0, 2.0), Vec3::new(1.0, 0.0, 6.0));
    let request = SpawnRequest {
        effect: catalog.effect_id("clipped").expect("clipped"),
        anchor: line.from,
        bound: BoundValues::default()
            .with(Binding::LineSpacing, 1.0)
            .with(Binding::LineLength, 4.0),
        line: Some(line),
        delay: 0.0,
    };
    runtime.spawn(&catalog, &request, 0.0);
    runtime.build_frame(&catalog, 0.0, None);
    runtime.build_frame(&catalog, 0.04, None);
    let cards = &runtime.frame.sorted;
    assert!(
        cards.len() >= 5 * 2,
        "two spines on each of five points: {}",
        cards.len()
    );
    for card in cards {
        assert_eq!(card.clip_axis, [0.0, 1.0], "clipped along the line (+z)");
        assert!((card.clip_min - 2.0).abs() < 1e-5 && (card.clip_max - 6.0).abs() < 1e-5);
    }
    let upright = cards
        .iter()
        .filter(|card| card.orient == client::effects::instances::ORIENT_UPRIGHT)
        .count();
    assert_eq!(upright, 10, "every spine card stands upright");
}

/// The shipped units with the first archetype that has modes laying `trail` while it moves in
/// its first mode, and no other presentation block; its id and that mode's index.
fn tables_with_trail(
    catalog: &EffectCatalog,
    trail: &str,
) -> Result<(UnitEffectTables, UnitTypeId, u8), EffectCatalogError> {
    let mut units = sim_fixtures::shipped_catalogs().units;
    let ids: Vec<UnitTypeId> = units
        .archetypes()
        .map(|archetype| archetype.unit_type_id)
        .collect();
    for other in &ids {
        units.get_mut(*other).expect("it exists").effects = ArchetypeEffects::default();
    }
    let (id, mode) = units
        .archetypes()
        .find_map(|archetype| {
            archetype
                .modes
                .first()
                .map(|mode| (archetype.unit_type_id, mode.mode_name.clone()))
        })
        .expect("a shipped archetype with a mode");
    units.get_mut(id).expect("it exists").effects = ArchetypeEffects {
        modes: vec![ModeEffects {
            mode,
            enter: None,
            leave: None,
            transition: None,
            stance: None,
            moving: Some(trail.to_owned()),
        }],
        ..ArchetypeEffects::default()
    };
    Ok((UnitEffectTables::build(&units, catalog)?, id, 0))
}

/// A fog for `viewer` on a 4 by 1 map whose cells 0 and 1 are in sight.
fn fog_for(viewer: PlayerId) -> BattleFog {
    use sim_types::VisibilityState::{Fogged, Visible};
    let view = PlayerView {
        viewer,
        map_width: 4,
        map_height: 1,
        visibility: vec![Visible, Visible, Fogged, Fogged],
        ..PlayerView::default()
    };
    let mut fog = BattleFog::default();
    fog.refill(SessionGeneration::FIRST, &view, std::iter::empty());
    fog
}

/// A burrowed unit of `owner` of type `unit` in mode `mode` at map `(x, 0.5)`, moving at
/// `speed` world units per second.
fn burrowed(
    owner: PlayerId,
    unit: UnitTypeId,
    mode: u8,
    x: f32,
    speed: f32,
) -> BubbleRenderInstance {
    use sim_types::{AnimState, EntityId, ShapeKind, SilhouetteClass, WeaponVisualKind};
    let mut bubble = BubbleRenderInstance {
        entity_id: EntityId(7),
        shape: ShapeKind::UnitSphere,
        silhouette: SilhouetteClass::HeavyHex,
        unit_type: unit,
        owner,
        world_pos: [x, 0.5, 0.0],
        ground_z: 0.0,
        radius: 0.5,
        velocity: [speed, 0.0],
        heading_angle_rad: 0.0,
        aim_angle_rad: 0.0,
        is_attacking: false,
        weapon_visual: WeaponVisualKind::Hitscan,
        attack_target_pos: None,
        anim_state: AnimState::Idle,
        attack_swing: None,
        hp_ratio: 1.0,
        shield_ratio: 1.0,
        shield_hit_flash: 0.0,
        hull_hit_angle_rad: 0.0,
        hull_hit_flash: 0.0,
        build_or_queue_progress: None,
        selected: false,
        concealment: Concealment::Burrowed,
        speed_boosted: false,
        stance: sim_types::StanceView::default(),
        status: sim_types::StatusFlags::default(),
        barrier_ratio: 0.0,
    };
    bubble.stance.mode = Some(mode);
    bubble
}

/// The frames of `seconds`, each 1/50 s long, as `(now, delta)`.
fn frames(seconds: u32) -> impl Iterator<Item = (f64, f64)> {
    (1..=seconds * 50).map(|frame| (f64::from(frame) * 0.02, 0.02))
}

/// How many trails `bubble` lays over `seconds` of 1/50 s frames.
fn trails_over(
    bubble: &BubbleRenderInstance,
    fog: Option<&BattleFog>,
    tables: &UnitEffectTables,
    catalog: &EffectCatalog,
    seconds: u32,
) -> usize {
    frames(seconds)
        .filter(|(now, delta)| trail_request(bubble, fog, tables, catalog, *now, *delta).is_some())
        .count()
}

/// Plan Amendment 2, R5: a burrowed enemy moving underground leaves a trail where the viewer
/// sees the ground, at its effect's period; standing still, or under fog, it leaves nothing.
/// The viewer's own burrowed unit leaves its trail anywhere.
#[test]
fn a_burrowed_unit_moving_underground_leaves_a_trail_where_the_ground_is_seen() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit, mode) = tables_with_trail(&catalog, "trail").expect("the trail resolves");
    assert!(tables.has_trails());
    let me = PlayerId(0);
    let them = PlayerId(1);
    let fog = fog_for(me);
    let moving = burrowed(them, unit, mode, 0.5, 2.0);
    assert_eq!(
        trails_over(&moving, Some(&fog), &tables, &catalog, 1),
        5,
        "one trail every 0.2 s while it moves in sight"
    );
    let request = frames(1)
        .find_map(|(now, delta)| trail_request(&moving, Some(&fog), &tables, &catalog, now, delta))
        .expect("a trail");
    assert!(
        (request.anchor - Vec3::new(0.5, 0.0, 0.5)).length() < 1e-5,
        "laid where the unit is, on the ground: {:?}",
        request.anchor
    );
    let still = burrowed(them, unit, mode, 0.5, 0.0);
    assert_eq!(
        trails_over(&still, Some(&fog), &tables, &catalog, 1),
        0,
        "a stationary burrowed enemy stays hidden"
    );
    let fogged = burrowed(them, unit, mode, 2.5, 2.0);
    assert_eq!(
        trails_over(&fogged, Some(&fog), &tables, &catalog, 1),
        0,
        "no trail on ground the viewer does not see"
    );
    let own = burrowed(me, unit, mode, 2.5, 2.0);
    assert_eq!(trails_over(&own, Some(&fog), &tables, &catalog, 1), 5);
    assert_eq!(
        trails_over(&fogged, None, &tables, &catalog, 1),
        5,
        "no fog"
    );
    let mut no_mode = moving.clone();
    no_mode.stance.mode = None;
    assert_eq!(
        trails_over(&no_mode, Some(&fog), &tables, &catalog, 1),
        0,
        "only in the mode that names the trail"
    );
    assert_eq!(
        fog.presence(&moving),
        Presence::Hidden,
        "the unit itself stays hidden while its trail shows"
    );
}

/// A `moving` slot needs an effect that states its period.
#[test]
fn a_trail_without_a_repeat_period_is_refused() {
    let (_dir, catalog) = catalog_with(|text| text);
    let Err(error) = tables_with_trail(&catalog, "trail_once") else {
        panic!("a trail with no period resolved");
    };
    assert!(
        error.to_string().contains("trail_once") && error.to_string().contains("repeat"),
        "{error}"
    );
}

/// Plan Amendment 2, R6: the fog admits an undetected burrowed attacker's line point by point,
/// so a line running out of the fog into sight draws exactly at the points in sight.
#[test]
fn the_fog_admits_a_line_point_by_point() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "spines");
    // The viewer sees cells 0 and 1; the line runs from x 3.6 (fogged) towards x 0.6.
    let fog = fog_for(PlayerId(0));
    let mut lines = LineAssembly::default();
    let mut ready = Vec::new();
    for index in 0..4_u8 {
        let event = CombatEventVisual::SplashVisual {
            center: [3.6 - f32::from(index), 0.5, 0.0],
            zones: [0.3; 3],
            line: Some(LineView {
                from: [4.6, 0.5, 0.0],
                to: [0.6, 0.5, 0.0],
                spacing: 1.0,
                index,
                count: 4,
            }),
            source: EffectSource {
                unit_type: unit,
                weapon: WeaponKey::default(),
            },
            owner: PlayerId(1),
        };
        if fog.reveals(&event) {
            let request = spawn_request(&event, &tables, &catalog).expect("the spines are named");
            ready.extend(lines.push(request));
        }
    }
    ready.extend(lines.finish());
    assert_eq!(
        ready.len(),
        1,
        "one spawn although the first point is fogged"
    );
    let line = ready[0].line.expect("a line");
    assert_eq!(
        line.seen & 0xF,
        0b1100,
        "points 2 and 3 (x 1.6 and 0.6) are in sight"
    );
    let mut runtime = EffectRuntime::new(&catalog);
    assert!(runtime.spawn(&catalog, &ready[0], 0.0).is_some());
    runtime.build_frame(&catalog, 0.95, Some(&camera()));
    let xs: Vec<f32> = runtime
        .frame
        .sorted
        .iter()
        .map(|card| card.center[0])
        .collect();
    assert_eq!(xs.len(), 2, "drawn at the two seen points only: {xs:?}");
    assert!(xs.iter().all(|x| *x < 2.0), "nothing in the fog: {xs:?}");
}

/// A line whose searches land one after another on several ticks (the spine line, B-85) is one
/// effect: a later search of the same line joins the live effect while its travel still lays
/// the line out, and a later attack along the very same line (a unit firing again from where
/// it stands) is a new effect.
#[test]
fn a_line_landing_over_several_ticks_is_one_effect_and_a_new_attack_another() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "spines");
    let mut runtime = EffectRuntime::new(&catalog);
    let request = |index: u8| {
        let event = splash(
            unit,
            1.0 + f32::from(index),
            0.0,
            [0.3, 0.3, 0.3],
            Some(LineView {
                from: [0.0, 0.0, 0.0],
                to: [9.0, 0.0, 0.0],
                spacing: 1.0,
                index,
                count: 9,
            }),
        );
        spawn_request(&event, &tables, &catalog).expect("the spines are named")
    };
    // One search per 0.08 s, each its own frame (travel 10: point k is due at 0.1 k s).
    assert!(runtime.spawn(&catalog, &request(0), 0.0).is_some());
    for index in 1..9_u8 {
        let now = 0.08 * f64::from(index);
        assert!(
            runtime.spawn(&catalog, &request(index), now).is_none(),
            "search {index} joins the live line"
        );
    }
    assert_eq!(runtime.stats.spawned, 1);
    assert_eq!(runtime.stats.merged_lines, 8);
    runtime.build_frame(&catalog, 0.95, Some(&camera()));
    assert_eq!(
        runtime.frame.sorted.len(),
        9,
        "every search point of the one effect is drawn"
    );
    // The same line again, after the first has laid it out: a new attack.
    assert!(runtime.spawn(&catalog, &request(0), 1.8).is_some());
    assert_eq!(runtime.stats.spawned, 2);
}

/// A line whose last search lies a double spacing beyond the one before (the flame line's
/// 6.0 then 6.5) is still one line in one frame: its points are merged by the far end they all
/// publish, not by the first point each recomputes from the spacing.
#[test]
fn an_unevenly_spaced_line_is_merged_by_its_far_end() {
    let (_dir, catalog) = catalog_with(|text| text);
    let (tables, unit) = tables_for(&catalog, "spines");
    let mut lines = LineAssembly::default();
    let mut ready = Vec::new();
    // Offsets 1, 2, 3 and then 5 (not 4) from the origin; spacing 1, far end 5.
    for (index, along) in [(0_u8, 1.0_f32), (1, 2.0), (2, 3.0), (3, 5.0)] {
        let event = splash(
            unit,
            along,
            0.0,
            [0.3, 0.3, 0.3],
            Some(LineView {
                from: [0.0, 0.0, 0.0],
                to: [5.0, 0.0, 0.0],
                spacing: 1.0,
                index,
                count: 4,
            }),
        );
        let request = spawn_request(&event, &tables, &catalog).expect("the spines are named");
        ready.extend(lines.push(request));
    }
    ready.extend(lines.finish());
    assert_eq!(ready.len(), 1, "one spawn for the line");
    assert!(
        (ready[0].anchor - Vec3::new(1.0, 0.0, 0.0)).length() < 1e-4,
        "anchored at the first search point: {:?}",
        ready[0].anchor
    );
    assert_eq!(ready[0].line.map(|line| line.seen & 0xF), Some(0xF));
}

/// WP9 review 2, E2-1: a status look first seen settled (a unit already in its status when
/// the viewer first sees it) draws its held layers as fully as one that came in.
#[test]
fn a_status_look_held_settled_draws_as_one_that_came_in() {
    use client::effects::binding::BoundValues;
    use client::effects::catalog::schema::Binding;
    use client::effects::runtime::SpawnRequest;
    let root = ResolvedAssetsRoot::resolve().expect("assets root");
    let catalog = EffectCatalog::load(&root.as_path().join("catalog/effects.ron"), root.as_path())
        .expect("the shipped vocabulary loads");
    let look = catalog
        .defaults
        .status
        .get(&sim_types::StatusKind::Barrier)
        .copied()
        .expect("a barrier look");
    let request = SpawnRequest {
        effect: look,
        anchor: Vec3::new(0.0, 0.5, 0.0),
        bound: BoundValues::default()
            .with(Binding::UnitRadius, 0.5)
            .with(Binding::UnitHeight, 1.0)
            .with(Binding::BarrierRatio, 1.0),
        line: None,
        delay: 0.0,
    };
    let strongest = |settled: bool| {
        let mut runtime = EffectRuntime::new(&catalog);
        runtime
            .hold(&catalog, &request, 100.0, settled)
            .expect("held");
        runtime.build_frame(&catalog, 101.0, Some(&camera()));
        runtime
            .frame
            .sorted
            .iter()
            .map(|card| card.intensity)
            .fold(0.0_f32, f32::max)
    };
    let (came_in, settled) = (strongest(false), strongest(true));
    assert!(came_in > 0.5, "the look that came in draws: {came_in}");
    assert!(
        (settled - came_in).abs() < 1e-3,
        "settled {settled} against came in {came_in}"
    );
}

/// WP10 review 1, E1-2: an enemy area the viewer stops seeing is cut to nothing on that frame,
/// with every effect it laid, and released; while it is seen in part, what it laid follows its
/// current clip.
#[test]
fn an_area_the_viewer_stops_seeing_is_cut_to_nothing_at_once_with_what_it_laid() {
    use client::effects::areas::{
        area_sight, keep_views_seen, AreaLooks, AreaSight, Keep, CLIP_NOTHING,
    };
    let dir = tempfile::tempdir().expect("a temporary directory");
    let manifest = write_fixture_set(dir.path());
    let effects = r#"
        "cloud": (class: Major, event: Area, emit: Some((effect: "puff_area", every: 0.05)), layers: [
            (block: Decal(texture: "soft_disc", ramp: "blue"), size: (bind: AreaRadius, scale: 1.0),
             duration: 0.1, held: true, exit: 0.4, priority: Critical),
        ]),
        "puff_area": (class: Minor, event: Area, layers: [
            (block: Sprite(texture: "soft_disc", ramp: "white"), size: (bind: AreaRadius, scale: 0.3),
             duration: 1.0),
        ])"#;
    let text = vocabulary(&manifest, "", effects, "");
    let catalog = EffectCatalog::from_text(&text, &dir.path().join("effects.ron"), dir.path())
        .expect("the vocabulary loads");
    let mut units = sim_fixtures::shipped_catalogs().units;
    let ids: Vec<_> = units.archetypes().map(|arch| arch.unit_type_id).collect();
    for id in &ids {
        units.get_mut(*id).expect("the archetype exists").effects = ArchetypeEffects::default();
    }
    let caster = units
        .archetypes()
        .find(|arch| !arch.abilities.is_empty())
        .map(|arch| arch.unit_type_id)
        .expect("a shipped unit has an ability");
    let archetype = units.get_mut(caster).expect("the archetype exists");
    let name = archetype.abilities[0].name.clone();
    archetype.effects.abilities = [(name, "cloud".to_owned())].into_iter().collect();
    let tables = UnitEffectTables::build(&units, &catalog).expect("the block resolves");
    let mut runtime = EffectRuntime::new(&catalog);
    let mut looks = AreaLooks::default();
    let area = sim_types::AreaView {
        id: 7,
        owner: PlayerId(1),
        center: [10.2, 10.4, 0.0],
        radius: 1.5,
        remaining_ticks: 60,
        total_ticks: 61,
        source: Some(sim_types::AbilitySource {
            unit_type: caster,
            ability: 0,
        }),
    };
    let sight = area_sight(bevy::math::Vec2::new(10.2, 10.4), 1.5, |x, _| {
        x.floor() >= 10.0
    });
    let AreaSight::Part(clip) = sight else {
        panic!("the area straddles the fog's edge: {sight:?}");
    };
    let frame = |looks: &mut AreaLooks, runtime: &mut EffectRuntime, index: u32, seen| {
        let mut keep = Keep {
            catalog: &catalog,
            tables: &tables,
            runtime,
            now: f64::from(index) * 0.04,
            delta: 0.04,
        };
        keep_views_seen(looks, (&[area], &[]), &[], |_| seen, |_| true, &mut keep);
    };
    for index in 1..=10 {
        frame(&mut looks, &mut runtime, index, sight);
    }
    assert_eq!(looks.held(), (1, 0));
    let clips: Vec<_> = runtime
        .pool
        .iter()
        .map(|(_, instance)| instance.fog_clip)
        .collect();
    assert!(
        clips.len() >= 4,
        "the area and what it laid live: {}",
        clips.len()
    );
    assert!(
        clips.iter().all(|live| *live == clip),
        "what the area laid follows its clip: {clips:?}"
    );
    frame(&mut looks, &mut runtime, 11, AreaSight::Hidden);
    assert_eq!(looks.held(), (0, 0), "the unseen area is released");
    assert!(
        runtime
            .pool
            .iter()
            .all(|(_, instance)| instance.fog_clip == CLIP_NOTHING),
        "nothing of it is drawn from the frame the viewer stops seeing it"
    );
}
