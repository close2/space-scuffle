//! The effect vocabulary loader through its public API (plan
//! `2026-11-09-effect-visualization` WP4, §5.2, §5.3, D-12).
//!
//! # Responsibility
//! One broken fixture per [`EffectCatalogError`] variant, each checking the variant and that
//! it names the file, effect and layer; the shipped vocabulary and every shipped unit's
//! presentation block resolve from the runtime path; and a copy of `effects.ron` edited in a
//! temporary directory loads a different effect with no rebuild (charter C6); and the
//! coverage rule of §5.3: every shipped splash weapon of a unit with a presentation block names
//! a splash effect whose `Critical` layers bind the reach or the line (the units without a
//! block, which fall back to the defaults, are reported), checked over the shipped blocks and
//! the lab-only entries; the slice's effects the review gate accepted (plan Amendment 3) are
//! named by their shipped units, and the effects only the lab-only entries name stay defined but
//! are named by no shipped unit.
//!
//! # Invariants verified
//! - Every failure is typed and names where it is.
//! - Names resolve to dense ids in file order; effect lifetimes cover every layer.

#![allow(clippy::expect_used, clippy::panic)]

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

#[allow(
    dead_code,
    reason = "shared with the effect GPU tests, which use all of it"
)]
#[path = "support/effect_subjects.rs"]
mod effect_subjects;

use client::effects::catalog::schema::{Binding, EventKind};
use client::effects::catalog::{
    Block, EffectCatalog, EffectCatalogError, NameKind, UnitEffectTables, EFFECTS_CATALOG_PATH,
};
use client::ResolvedAssetsRoot;
use effect_fixtures::{vocabulary, write_fixture_set, write_named_set};
use sim_core::catalog::{ArchetypeEffects, WeaponEffects};
use sim_types::{EffectSource, WeaponKey, WeaponSlot};
use std::path::{Path, PathBuf};

/// A temporary directory with the fixture set written into it.
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let manifest = write_fixture_set(dir.path());
    (dir, manifest)
}

/// Loads `text` as if read from `vocabulary.ron` in `dir`.
fn load(dir: &Path, text: &str) -> Result<EffectCatalog, EffectCatalogError> {
    EffectCatalog::from_text(text, &dir.join("vocabulary.ron"), dir)
}

/// A one-layer splash effect named `boom` whose layer is `layer`.
fn splash_effect(layer: &str) -> String {
    format!(r#""boom": (class: Major, event: Splash, layers: [ {layer} ])"#)
}

/// A valid sprite layer.
const SPRITE: &str = r#"(block: Sprite(texture: "soft_disc", ramp: "white"),
    size: (bind: SplashReach, scale: 1.0), duration: 0.5, curve: "fade", priority: Critical)"#;

/// Asserts the error's text names `vocabulary.ron` and every needle.
fn assert_names(error: &EffectCatalogError, needles: &[&str]) {
    let text = error.to_string();
    for needle in needles {
        assert!(text.contains(needle), "'{text}' should name {needle}");
    }
}

#[test]
fn a_valid_vocabulary_resolves_every_name_to_a_dense_id() {
    let (dir, manifest) = fixture();
    let effects = format!(
        r#"{}, "puff": (class: Minor, event: Point, layers: [
            (block: Particles(emitter: "sparks"), priority: Detail),
            (block: Ring(texture: "soft_disc", ramp: "green", thickness: 0.2),
             radius: (from: (scale: 0.5), to: (scale: 2.0)), duration: 0.3),
            (block: Decal(texture: "solid", ramp: "blue", erosion: "fade"), size: (scale: 1.0),
             start: 0.1, duration: 4.0),
            (block: Light(color: (1.0, 0.5, 0.2), intensity: 800.0), size: (scale: 3.0),
             duration: 0.2),
        ])"#,
        splash_effect(SPRITE)
    );
    let emitters = r#""sparks": (spawn: Burst(count: (base: 8, max: 16)), speed: (1.0, 2.0),
        cone_deg: 30.0, lifetime: (0.3, 0.6), size: (0.05, 0.1), texture: "soft_disc",
        orient: VelocityStretched(1.0), ramp: "red_add")"#;
    let catalog = load(
        dir.path(),
        &vocabulary(&manifest, emitters, &effects, r#"impact: "puff""#),
    )
    .expect("the vocabulary is valid");
    assert_eq!(catalog.effects.len(), 2);
    let boom = catalog.effect_id("boom").expect("boom resolves");
    let puff = catalog.effect_id("puff").expect("puff resolves");
    assert_eq!((boom.0, puff.0), (0, 1), "ids follow name order");
    assert_eq!(catalog.defaults.impact, Some(puff));
    let puff = catalog.effect(puff).expect("puff exists");
    assert!(matches!(puff.layers[0].block, Block::Particles(_)));
    assert!(
        (puff.layers[0].duration - 0.6).abs() < 1e-6,
        "a burst lives its longest particle"
    );
    assert!((puff.lifetime - 4.1).abs() < 1e-5, "the decal ends last");
    assert_eq!(catalog.atlas.layers, 7);
    assert_eq!(catalog.atlas.cell_size, effect_fixtures::CELL);
    assert_ne!(catalog.atlas.texels.len(), 0);
    assert_eq!(catalog.ramps.len(), 4);
}

#[test]
fn a_missing_file_is_an_io_error() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let error =
        EffectCatalog::load(&dir.path().join("absent.ron"), dir.path()).expect_err("no file");
    assert!(matches!(error, EffectCatalogError::Io { .. }), "{error}");
    assert_names(&error, &["absent.ron"]);
}

#[test]
fn an_unknown_field_is_a_parse_error() {
    let (dir, manifest) = fixture();
    let text = vocabulary(&manifest, "", "", "")
        .replace("format_version: 1", "format_version: 1, colour: 3");
    let error = load(dir.path(), &text).expect_err("unknown field");
    assert!(matches!(error, EffectCatalogError::Parse { .. }), "{error}");
    assert_names(&error, &["vocabulary.ron", "colour"]);
}

#[test]
fn another_format_version_is_refused() {
    let (dir, manifest) = fixture();
    let text = vocabulary(&manifest, "", "", "").replace("format_version: 1", "format_version: 7");
    let error = load(dir.path(), &text).expect_err("version");
    assert!(
        matches!(
            error,
            EffectCatalogError::FormatVersion {
                found: 7,
                expected: 1,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn unknown_ramps_curves_emitters_and_effects_name_their_site() {
    let (dir, manifest) = fixture();
    let cases = [
        (
            SPRITE.replace("\"white\"", "\"mauve\""),
            NameKind::Ramp,
            "mauve",
        ),
        (
            SPRITE.replace("\"fade\"", "\"wobble\""),
            NameKind::Curve,
            "wobble",
        ),
        (
            r#"(block: Particles(emitter: "ghost"), priority: Detail)"#.to_owned(),
            NameKind::Emitter,
            "ghost",
        ),
    ];
    for (layer, kind, name) in cases {
        let text = vocabulary(&manifest, "", &splash_effect(&layer), "");
        let error = load(dir.path(), &text).expect_err("unknown name");
        match &error {
            EffectCatalogError::UnknownName {
                site,
                kind: found,
                name: found_name,
                ..
            } => {
                assert_eq!((*found, found_name.as_str()), (kind, name));
                assert_eq!((site.owner.as_str(), site.layer), ("boom", Some(0)));
            }
            other => panic!("expected UnknownName, got {other}"),
        }
        assert_names(&error, &["vocabulary.ron", "'boom' layer 0", name]);
    }
    let text = vocabulary(&manifest, "", &splash_effect(SPRITE), r#"death: "nothing""#);
    let error = load(dir.path(), &text).expect_err("unknown default");
    assert!(
        matches!(
            &error,
            EffectCatalogError::UnknownName {
                kind: NameKind::Effect,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_texture_element_missing_from_every_set_is_refused() {
    let (dir, manifest) = fixture();
    let layer = SPRITE.replace("soft_disc", "fireball");
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&layer), ""),
    )
    .expect_err("missing element");
    assert!(
        matches!(&error, EffectCatalogError::MissingTextureElement { element, .. } if element == "fireball"),
        "{error}"
    );
    assert_names(&error, &["'boom' layer 0"]);
}

#[test]
fn a_data_element_cannot_be_drawn() {
    let (dir, manifest) = fixture();
    let layer = SPRITE.replace("soft_disc", "data_sheet");
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&layer), ""),
    )
    .expect_err("data element");
    assert!(
        matches!(&error, EffectCatalogError::UndrawableElement { .. }),
        "{error}"
    );
}

#[test]
fn broken_texture_sets_are_refused_with_their_file() {
    let (dir, manifest) = fixture();
    let listed_twice = vocabulary(&manifest, "", "", "")
        .replace("textures: [", &format!("textures: [{manifest:?}, "));
    let error = load(dir.path(), &listed_twice).expect_err("duplicate element");
    assert!(
        matches!(&error, EffectCatalogError::DuplicateElement { .. }),
        "{error}"
    );

    let absent = dir.path().join("absent.texture.ron");
    let error = load(dir.path(), &vocabulary(&absent, "", "", "")).expect_err("no manifest");
    assert!(
        matches!(&error, EffectCatalogError::TextureManifest { .. }),
        "{error}"
    );
    assert_names(&error, &["absent.texture.ron"]);

    let small = write_named_set(dir.path(), "fx_small", 16, 3, "small_");
    let mixed = vocabulary(&manifest, "", "", "")
        .replace("textures: [", &format!("textures: [{small:?}, "));
    let error = load(dir.path(), &mixed).expect_err("cell sizes differ");
    assert!(
        matches!(
            &error,
            EffectCatalogError::CellSizeMismatch {
                found: 32,
                expected: 16,
                ..
            }
        ),
        "{error}"
    );

    image::RgbaImage::new(8, 8)
        .save(dir.path().join("fx_fixture.mip1.png"))
        .expect("a wrong-size strip is written");
    let error = load(dir.path(), &vocabulary(&manifest, "", "", "")).expect_err("strip size");
    assert!(
        matches!(&error, EffectCatalogError::StripSize { .. }),
        "{error}"
    );

    std::fs::write(dir.path().join("fx_fixture.mip1.png"), b"not a png")
        .expect("a corrupt strip is written");
    let error = load(dir.path(), &vocabulary(&manifest, "", "", "")).expect_err("corrupt strip");
    assert!(
        matches!(&error, EffectCatalogError::TextureImage { .. }),
        "{error}"
    );
}

#[test]
fn a_binding_the_event_kind_does_not_provide_is_refused() {
    let (dir, manifest) = fixture();
    let effect = format!(r#""pop": (class: Minor, event: Death, layers: [ {SPRITE} ])"#);
    let error = load(dir.path(), &vocabulary(&manifest, "", &effect, "")).expect_err("binding");
    match &error {
        EffectCatalogError::UnboundBinding {
            binding,
            event,
            site,
            ..
        } => {
            assert_eq!((*binding, *event), (Binding::SplashReach, EventKind::Death));
            assert_eq!((site.owner.as_str(), site.layer), ("pop", Some(0)));
        }
        other => panic!("expected UnboundBinding, got {other}"),
    }
    let line = SPRITE.replace(
        "priority: Critical",
        "priority: Critical, placement: Line(spacing: (scale: 1.0))",
    );
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&line), ""),
    )
    .expect_err("a line placement needs a line event");
    assert!(
        matches!(
            &error,
            EffectCatalogError::UnboundBinding {
                binding: Binding::LineSpacing,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn unordered_curve_and_ramp_keys_are_refused() {
    let (dir, manifest) = fixture();
    let text = vocabulary(&manifest, "", "", "").replace(
        r#""fade": [(0.0, 1.0, Linear), (1.0, 0.0, Linear)]"#,
        r#""fade": [(0.0, 1.0, Linear), (0.7, 0.5, Linear), (0.4, 0.0, Linear)]"#,
    );
    let error = load(dir.path(), &text).expect_err("unordered curve");
    match &error {
        EffectCatalogError::UnorderedKeys { site, index, .. } => {
            assert_eq!((site.owner.as_str(), *index), ("fade", 2));
        }
        other => panic!("expected UnorderedKeys, got {other}"),
    }
    let text = vocabulary(&manifest, "", "", "").replace(
        r#""blue": [(0.0, (0.0, 0.0, 1.0, 1.0)), (1.0"#,
        r#""blue": [(0.5, (0.0, 0.0, 1.0, 1.0)), (0.2"#,
    );
    let error = load(dir.path(), &text).expect_err("unordered ramp");
    assert!(
        matches!(&error, EffectCatalogError::UnorderedKeys { .. }),
        "{error}"
    ); // D-11 (WP6c): no HDR, so a ramp channel above 1.0 is refused, naming ramp and key.
    let text = vocabulary(&manifest, "", "", "").replace(
        r#""blue": [(0.0, (0.0, 0.0, 1.0, 1.0)), (1.0"#,
        r#""blue": [(0.0, (0.0, 0.0, 1.5, 1.0)), (1.0"#,
    );
    let error = load(dir.path(), &text).expect_err("ramp above 1.0");
    match &error {
        EffectCatalogError::RampOutOfRange {
            ramp, index, value, ..
        } => assert_eq!((ramp.as_str(), *index, *value), ("blue", 0, 1.5)),
        other => panic!("expected RampOutOfRange, got {other}"),
    }
}

#[test]
fn the_d12_limits_are_load_time_errors() {
    let (dir, manifest) = fixture();
    let thirteen = vec![SPRITE; 13].join(", ");
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&thirteen), ""),
    )
    .expect_err("13 layers");
    assert!(
        matches!(
            &error,
            EffectCatalogError::TooManyLayers {
                count: 13,
                limit: 12,
                ..
            }
        ),
        "{error}"
    );
    let five = SPRITE.replace(
        "priority: Critical",
        "priority: Critical, placements: [Point, Point, Point, Point, Point]",
    );
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&five), ""),
    )
    .expect_err("5 placements");
    assert!(
        matches!(
            &error,
            EffectCatalogError::TooManyPlacements {
                count: 5,
                limit: 4,
                ..
            }
        ),
        "{error}"
    );
    let forty = SPRITE.replace(
        "priority: Critical",
        "priority: Critical, placement: Disc(count: 40, radius: (scale: 1.0))",
    );
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&forty), ""),
    )
    .expect_err("40 instances");
    assert!(
        matches!(
            &error,
            EffectCatalogError::TooManyInstances {
                count: 40,
                limit: 32,
                ..
            }
        ),
        "{error}"
    );
}

/// Plan Amendment 2: a liquid decal's material, a decal's clip feather and an effect's repeat
/// period are range-checked at load, naming the field.
#[test]
fn liquid_feather_and_repeat_values_are_range_checked() {
    let (dir, manifest) = fixture();
    let decal = |extra: &str| {
        format!(
            r#"(block: Decal(texture: "solid", ramp: "blue"{extra}), size: (scale: 1.0), duration: 1.0)"#
        )
    };
    let liquid = |relief: f32, sheen: f32, gloss: f32, noise: f32, dry: f32| {
        decal(&format!(
            ", liquid: (relief: {relief}, sheen: {sheen}, gloss: {gloss}, value_noise: {noise}, dry_below: {dry})"
        ))
    };
    let good = liquid(0.05, 1.0, 20.0, 0.5, 0.5);
    load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&good), ""),
    )
    .expect("a valid liquid decal loads");
    load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&decal(", width: 4.0")), ""),
    )
    .expect("a decal four times as wide as long loads (review round 4)");
    let outline =
        |noise: f32, fill: f32| decal(&format!(", outline: (edge_noise: {noise}, fill: {fill})"));
    load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&outline(0.02, 0.76)), ""),
    )
    .expect("a decal with a disc outline loads (review round 5)");
    for (layer, field) in [
        (outline(-0.01, 0.76), "outline.edge_noise"),
        (outline(0.3, 0.76), "outline.edge_noise"),
        (outline(0.02, 0.2), "outline.fill"),
        (outline(0.02, 1.1), "outline.fill"),
        (liquid(0.0, 1.0, 20.0, 0.5, 0.5), "liquid.relief"),
        (liquid(0.05, 5.0, 20.0, 0.5, 0.5), "liquid.sheen"),
        (liquid(0.05, 1.0, 0.5, 0.5, 0.5), "liquid.gloss"),
        (liquid(0.05, 1.0, 20.0, 1.5, 0.5), "liquid.value_noise"),
        (liquid(0.05, 1.0, 20.0, 0.5, 1.0), "liquid.dry_below"),
        (decal(", clip_feather: 0.0"), "clip_feather"),
        (decal(", width: 0.0"), "width"),
        (decal(", width: 4.5"), "width"),
    ] {
        let error = load(
            dir.path(),
            &vocabulary(&manifest, "", &splash_effect(&layer), ""),
        )
        .expect_err(field);
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: found, .. } if *found == field),
            "{field}: {error}"
        );
    }
    let repeating = format!(
        r#""boom": (class: Minor, event: Mode, repeat: Some(-1.0), layers: [ {} ])"#,
        decal("")
    );
    let error = load(dir.path(), &vocabulary(&manifest, "", &repeating, ""))
        .expect_err("a negative repeat period");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue {
                field: "repeat",
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn out_of_range_values_and_empty_pools_are_refused() {
    let (dir, manifest) = fixture();
    let negative = SPRITE.replace("duration: 0.5", "duration: -1.0");
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&negative), ""),
    )
    .expect_err("negative duration");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue {
                field: "duration",
                ..
            }
        ),
        "{error}"
    );
    let decal =
        r#"(block: Decal(texture: "solid", ramp: "blue"), size: (scale: 1.0), duration: 1.0)"#;
    let text =
        vocabulary(&manifest, "", &splash_effect(decal), "").replace("decals: 64", "decals: 0");
    let error = load(dir.path(), &text).expect_err("no decal capacity");
    assert!(
        matches!(
            &error,
            EffectCatalogError::NoCapacity {
                block: "decals",
                ..
            }
        ),
        "{error}"
    );
}

/// A ribbon (WP7) spans its event's line once: it needs a `Line` event, only the `Point`
/// placement, a camera-facing or ground orientation and ranged values; a valid one resolves.
#[test]
fn a_ribbon_needs_a_line_one_placement_and_ranged_values() {
    let (dir, manifest) = fixture();
    let ribbon = |block: &str, extra: &str| {
        format!(
            r#"(block: Ribbon(texture: "solid", ramp: "white"{block}), size: (scale: 0.2),
            duration: 0.5, priority: Critical{extra})"#
        )
    };
    let line_effect =
        |layer: String| format!(r#""jet": (class: Major, event: Line, layers: [ {layer} ])"#);
    let catalog = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &line_effect(ribbon(
                ", orient: Ground, travel: 20.0, tail_delay: Some(0.1), taper: (0.1, 0.3), shape: Radial",
                "",
            )),
            "",
        ),
    )
    .expect("a valid ribbon resolves");
    let effect = catalog
        .effect(catalog.effect_id("jet").expect("defined"))
        .expect("resolves");
    match &effect.layers[0].block {
        Block::Ribbon(look) => {
            assert!(look.ground);
            assert_eq!(look.taper, (0.1, 0.3));
            assert_eq!(look.tail_delay, Some(0.1));
        }
        other => panic!("expected a ribbon, got {other:?}"),
    }
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(&ribbon("", "")), ""),
    )
    .expect_err("a ribbon needs a line event");
    assert!(
        matches!(
            &error,
            EffectCatalogError::UnboundBinding {
                binding: Binding::LineLength,
                event: EventKind::Splash,
                ..
            }
        ),
        "{error}"
    );
    for (block, extra, field) in [
        (
            "",
            ", placement: Line(spacing: (bind: LineSpacing, scale: 1.0))",
            "placement",
        ),
        (", orient: Upright", "", "orient"),
        (", taper: (0.6, 0.1)", "", "taper"),
        (", repeat: 0.0", "", "repeat"),
        (", travel: -1.0", "", "travel"),
        (", tail_delay: Some(-0.5)", "", "tail_delay"),
    ] {
        let error = load(
            dir.path(),
            &vocabulary(&manifest, "", &line_effect(ribbon(block, extra)), ""),
        )
        .expect_err(field);
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: named, .. } if *named == field),
            "{field}: {error}"
        );
    }
}

#[test]
fn the_shipped_vocabulary_and_presentation_blocks_resolve_from_the_runtime_path() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    UnitEffectTables::build(&units, &catalog).expect("every shipped presentation block resolves");
    assert!(catalog.budget.effects > 0 && catalog.budget.sprites > 0);
}

/// The shipped unit catalog with the first archetype's presentation block replaced and every
/// other block emptied.
fn units_with_block(
    block: ArchetypeEffects,
) -> (sim_core::catalog::UnitCatalog, sim_types::UnitTypeId) {
    let mut units = sim_fixtures::shipped_catalogs().units;
    let ids: Vec<_> = units
        .archetypes()
        .map(|archetype| archetype.unit_type_id)
        .collect();
    let id = *ids.first().expect("the shipped catalog has an archetype");
    // Every other block names shipped effects the fixture vocabulary does not define.
    for other in &ids {
        units.get_mut(*other).expect("the archetype exists").effects = ArchetypeEffects::default();
    }
    units.get_mut(id).expect("the archetype exists").effects = block;
    (units, id)
}

/// A presentation block naming `splash` for the primary weapon.
fn block_with_splash(splash: &str, mode: Option<&str>) -> ArchetypeEffects {
    ArchetypeEffects {
        weapons: vec![WeaponEffects {
            weapon: WeaponSlot::Primary,
            mode: mode.map(str::to_owned),
            launch: None,
            travel: None,
            impact: None,
            splash: Some(splash.to_owned()),
        }],
        ..ArchetypeEffects::default()
    }
}

#[test]
fn presentation_blocks_resolve_into_dense_tables_and_name_their_errors() {
    let (dir, manifest) = fixture();
    let point = format!(
        r#""hit": (class: Minor, event: Point, layers: [ {} ])"#,
        SPRITE.replace("bind: SplashReach, ", "")
    );
    let catalog = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &format!("{}, {point}", splash_effect(SPRITE)),
            "",
        ),
    )
    .expect("valid vocabulary");

    let (units, id) = units_with_block(block_with_splash("boom", None));
    let tables = UnitEffectTables::build(&units, &catalog).expect("the block resolves");
    let source = EffectSource {
        unit_type: id,
        weapon: WeaponKey::default(),
    };
    assert_eq!(tables.splash(source), catalog.effect_id("boom"));
    let other_slot = EffectSource {
        weapon: WeaponKey {
            slot: WeaponSlot::Air,
            mode: None,
        },
        ..source
    };
    assert_eq!(tables.splash(other_slot), None, "no default splash");

    let (units, _) = units_with_block(block_with_splash("kaboom", None));
    let error = UnitEffectTables::build(&units, &catalog).expect_err("unknown effect");
    assert!(
        matches!(&error, EffectCatalogError::UnknownUnitEffect { name, .. } if name == "kaboom"),
        "{error}"
    );
    let (units, _) = units_with_block(block_with_splash("boom", Some("no_such_mode")));
    let error = UnitEffectTables::build(&units, &catalog).expect_err("unknown mode");
    assert!(
        matches!(&error, EffectCatalogError::UnknownUnitMode { .. }),
        "{error}"
    );
    let (units, _) = units_with_block(block_with_splash("hit", None));
    let error = UnitEffectTables::build(&units, &catalog).expect_err("wrong kind");
    assert!(
        matches!(
            &error,
            EffectCatalogError::WrongEventKind {
                found: EventKind::Point,
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn an_edited_copy_of_the_shipped_vocabulary_loads_a_different_effect_without_a_rebuild() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let shipped = std::fs::read_to_string(root.as_path().join(EFFECTS_CATALOG_PATH))
        .expect("the shipped vocabulary is readable");
    let dir = tempfile::tempdir().expect("a temporary directory");
    let copy = dir.path().join("effects.ron");
    std::fs::write(&copy, &shipped).expect("the copy is written");
    let before = EffectCatalog::load(&copy, root.as_path()).expect("the copy loads");
    assert!(before.effect_id("boom").is_none());
    let element = before
        .atlas
        .elements
        .iter()
        .find(|(_, element)| element.drawable)
        .map(|(name, _)| name.clone())
        .expect("the shipped sets have a drawable element");

    let edited = shipped
        .replacen(
            "ramps: {",
            r#"ramps: { "white": [(0.0, (1.0, 1.0, 1.0, 1.0)), (1.0, (1.0, 1.0, 1.0, 1.0))],"#,
            1,
        )
        .replacen(
            "effects: {",
            &format!(
                r#"effects: {{ "boom": (class: Major, event: Splash, layers: [
                (block: Sprite(texture: "{element}", ramp: "white"),
                 size: (bind: SplashReach, scale: 2.5), duration: 0.75, priority: Critical) ]),"#
            ),
            1,
        );
    assert_ne!(edited, shipped, "the edit applies to the shipped text");
    std::fs::write(&copy, &edited).expect("the edit is written");
    let after = EffectCatalog::load(&copy, root.as_path()).expect("the edited copy loads");
    let boom = after
        .effect(after.effect_id("boom").expect("the new effect is there"))
        .expect("it resolves");
    assert!((boom.layers[0].size.scale - 2.5).abs() < 1e-6);
    assert!((boom.lifetime - 0.75).abs() < 1e-6);
}

/// The splash weapons of `archetype`: slot, mode and spec, the mode overrides under the slot
/// the simulation tags them with.
fn splash_weapons(
    archetype: &sim_core::catalog::UnitArchetype,
) -> Vec<(WeaponSlot, Option<String>)> {
    let ground_slot = if archetype.ground_weapon == archetype.weapon {
        WeaponSlot::Primary
    } else {
        WeaponSlot::Ground
    };
    let mut weapons = Vec::new();
    let has_splash = |weapon: Option<&sim_types::WeaponSpec>| {
        weapon.is_some_and(|weapon| weapon.splash.is_some())
    };
    if has_splash(archetype.weapon.as_ref()) {
        weapons.push((WeaponSlot::Primary, None));
    }
    if ground_slot == WeaponSlot::Ground && has_splash(archetype.ground_weapon.as_ref()) {
        weapons.push((WeaponSlot::Ground, None));
    }
    if has_splash(archetype.air_weapon.as_ref()) {
        weapons.push((WeaponSlot::Air, None));
    }
    for mode in &archetype.modes {
        if has_splash(
            mode.ground_weapon_override
                .as_ref()
                .and_then(Option::as_ref),
        ) {
            weapons.push((ground_slot, Some(mode.mode_name.clone())));
        }
    }
    weapons
}

/// Whether `layer` binds the reach of its splash or its line.
fn binds_extent(layer: &client::effects::catalog::Layer) -> bool {
    let extent = |binding: Binding| {
        matches!(
            binding,
            Binding::SplashReach | Binding::LineLength | Binding::LineSpacing
        )
    };
    let placed = layer.placements.iter().any(|placement| match placement {
        client::effects::catalog::schema::PlacementDef::Line { spacing, .. } => {
            extent(spacing.bind)
        }
        _ => false,
    });
    // A ribbon spans its event's line by construction (WP7).
    let spans_line = matches!(layer.block, Block::Ribbon(_));
    extent(layer.size.bind)
        || layer.radius.is_some_and(|radius| extent(radius.to.bind))
        || placed
        || spans_line
}

#[test]
fn every_shipped_splash_weapon_with_a_block_names_a_splash_effect_bound_to_its_extent() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    // The shipped blocks plus the lab-only entries: the rule holds for both.
    let units = effect_subjects::lab_units();
    let mut covered = Vec::new();
    let mut defaulted = Vec::new();
    for archetype in units.archetypes() {
        for (slot, mode) in splash_weapons(archetype) {
            let label = format!("{} {slot:?} {mode:?}", archetype.slug);
            if archetype.effects.is_empty() {
                defaulted.push(label);
                continue;
            }
            let entry = archetype
                .effects
                .weapons
                .iter()
                .find(|entry| entry.weapon == slot && entry.mode == mode);
            let name = entry
                .and_then(|entry| entry.splash.as_ref())
                .unwrap_or_else(|| panic!("{label}: the block names no splash effect"));
            let effect = catalog
                .effect_id(name)
                .and_then(|id| catalog.effect(id))
                .unwrap_or_else(|| panic!("{label}: {name} does not resolve"));
            assert!(
                matches!(effect.event, EventKind::Splash | EventKind::Line),
                "{label}: {name} is not a splash effect"
            );
            let critical: Vec<_> = effect
                .layers
                .iter()
                .filter(|layer| {
                    layer.priority == client::effects::catalog::schema::Priority::Critical
                })
                .collect();
            assert!(
                !critical.is_empty(),
                "{label}: {name} has no Critical layer"
            );
            for layer in critical {
                assert!(
                    binds_extent(layer),
                    "{label}: a Critical layer of {name} binds no extent"
                );
            }
            covered.push(format!("{label} -> {name}"));
        }
    }
    assert!(
        !covered.is_empty(),
        "the slice's units name their splash effects"
    );
    let subjects = effect_subjects::splash_subjects();
    assert_eq!(
        subjects.len(),
        covered
            .iter()
            .map(|line| line.rsplit(" -> ").next().unwrap_or_default())
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        "every named splash effect is a GPU-test subject"
    );
    eprintln!(
        "splash coverage: {} named\n  {}\nsplash weapons on the defaults (no presentation block): {}\n  {}",
        covered.len(),
        covered.join("\n  "),
        defaulted.len(),
        defaulted.join("\n  ")
    );
}

/// The slice's splash effects the review gate accepted in round 5 (plan Amendment 3): the unit
/// that names each, the mode whose weapon override carries it (`None` for the default stance)
/// and the effect. Test data restating the shipped catalog, so a block dropped from
/// `units.ron` fails here.
const ACCEPTED_SLICE: [(&str, Option<&str>, &str); 3] = [
    ("railgun_crawler", Some("siege_mode"), "shell_impact"),
    ("volatile_tick", None, "acid_burst"),
    ("burrow_scorpion", Some("burrowed"), "spine_line"),
];

/// Every effect the review gate accepted is named by its shipped unit's presentation block
/// (not by a lab-only entry), resolves, and is a GPU-test subject bound to that unit.
#[test]
fn accepted_slice_effects_are_named_by_their_shipped_units() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    UnitEffectTables::build(&units, &catalog).expect("every shipped presentation block resolves");
    let subjects = effect_subjects::splash_subjects();
    for (slug, mode, effect) in ACCEPTED_SLICE {
        let archetype = units
            .id_by_slug(slug)
            .and_then(|id| units.get(id).ok())
            .unwrap_or_else(|| panic!("{slug} is a shipped unit"));
        let named = archetype.effects.weapons.iter().any(|entry| {
            entry.weapon == WeaponSlot::Primary
                && entry.mode.as_deref() == mode
                && entry.splash.as_deref() == Some(effect)
        });
        assert!(named, "{slug} {mode:?} names {effect} in units.ron");
        assert!(
            catalog.effect_id(effect).is_some(),
            "{effect} is defined in the vocabulary"
        );
        let subject = subjects
            .iter()
            .find(|subject| subject.effect == effect)
            .unwrap_or_else(|| panic!("{effect} is a GPU-test subject"));
        assert_eq!(subject.unit, slug, "{effect} is bound to {slug}'s weapon");
    }
}

/// The effects only the lab-only entries name (a trail no shipped burrowed mode can use yet,
/// plan Amendment 2 R5) stay in the vocabulary and under test through those entries, but no
/// shipped unit names them, and the lab-only entries resolve against the shipped vocabulary.
#[test]
fn lab_only_effects_are_defined_and_named_by_no_shipped_unit() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let lab_only: std::collections::BTreeSet<String> = effect_subjects::lab_blocks()
        .values()
        .flat_map(named_effects)
        .collect();
    assert!(!lab_only.is_empty(), "the fixture names lab-only effects");
    for name in &lab_only {
        assert!(
            catalog.effect_id(name).is_some(),
            "{name} stays defined in the vocabulary"
        );
    }
    let units = sim_fixtures::shipped_catalogs().units;
    for archetype in units.archetypes() {
        for named in named_effects(&archetype.effects) {
            assert!(
                !lab_only.contains(&named),
                "{} names the lab-only effect {named}",
                archetype.slug
            );
        }
    }
    UnitEffectTables::build(&effect_subjects::lab_units(), &catalog)
        .expect("every lab-only presentation entry resolves");
}

/// Every effect name a presentation block names, in any slot.
fn named_effects(block: &ArchetypeEffects) -> Vec<String> {
    let weapons = block.weapons.iter().flat_map(|entry| {
        [&entry.launch, &entry.travel, &entry.impact, &entry.splash]
            .into_iter()
            .flatten()
    });
    let modes = block.modes.iter().flat_map(|entry| {
        [
            &entry.enter,
            &entry.leave,
            &entry.transition,
            &entry.stance,
            &entry.moving,
        ]
        .into_iter()
        .flatten()
    });
    weapons
        .chain(modes)
        .chain(block.status.values())
        .chain(block.abilities.values())
        .chain(block.death.iter())
        .cloned()
        .collect()
}

/// A fixture directory with the fixture set and the noise set; returns both manifests.
fn noise_fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let (dir, manifest) = fixture();
    let noise = effect_fixtures::write_noise_set(dir.path());
    (dir, manifest, noise)
}

#[test]
fn breakup_noise_resolves_to_its_layer_after_the_drawable_elements() {
    let (dir, manifest, noise) = noise_fixture();
    let layer = r#"(block: Sprite(texture: "soft_disc", ramp: "white", orient: Upright,
        lean_deg: 20.0, spread: 0.5, sort_bias: 1.0, shape: Radial,
        breakup: (noise: "grain", scale: 2.0, amount: 0.4, softness: 0.1, shade: 0.5)),
        size: (bind: SplashReach, scale: 1.0), duration: 0.5, priority: Critical)"#;
    let catalog = load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(layer)),
    )
    .expect("a breakup naming a noise element resolves");
    assert_eq!(
        catalog.atlas.layers, 8,
        "the noise layer is uploaded after the 7 drawable ones"
    );
    let boom = catalog
        .effect(catalog.effect_id("boom").expect("boom"))
        .expect("boom exists");
    let Block::Sprite(look) = &boom.layers[0].block else {
        panic!("a sprite");
    };
    let breakup = look.breakup.expect("the breakup resolved");
    assert_eq!(breakup.noise_layer, 7);
    assert!((breakup.amount - 0.4).abs() < f32::EPSILON);
    assert!((look.lean - 20.0_f32.to_radians()).abs() < 1e-6);
}

#[test]
fn a_breakup_must_name_a_noise_element() {
    let (dir, manifest, noise) = noise_fixture();
    let layer = SPRITE.replace(
        r#"ramp: "white")"#,
        r#"ramp: "white", breakup: (noise: "solid", amount: 0.3))"#,
    );
    let error = load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(&layer)),
    )
    .expect_err("a drawable element is not noise");
    assert!(
        matches!(&error, EffectCatalogError::NotNoiseElement { element, .. } if element == "solid"),
        "{error}"
    );
    assert_names(&error, &["'boom' layer 0"]);
    let grain = SPRITE.replace("soft_disc", "grain");
    let error = load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(&grain)),
    )
    .expect_err("a noise element is not drawable");
    assert!(
        matches!(&error, EffectCatalogError::UndrawableElement { .. }),
        "{error}"
    );
    let loud = SPRITE.replace(
        r#"ramp: "white")"#,
        r#"ramp: "white", breakup: (noise: "grain", amount: 1.5))"#,
    );
    let error = load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(&loud)),
    )
    .expect_err("an amount above 1");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue {
                field: "breakup.amount",
                ..
            }
        ),
        "{error}"
    );
}

#[test]
fn a_front_ring_needs_its_breakup_noise() {
    let (dir, manifest, noise) = noise_fixture();
    let ring = r#"(block: Ring(texture: "soft_disc", ramp: "green", thickness: 0.2, mapping: Front),
        radius: (from: (scale: 0.5), to: (bind: SplashReach, scale: 1.0)), duration: 0.3,
        priority: Critical)"#;
    let error = load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(ring)),
    )
    .expect_err("a front without noise");
    assert!(
        matches!(&error, EffectCatalogError::FrontWithoutBreakup { .. }),
        "{error}"
    );
    assert_names(&error, &["'boom' layer 0"]);
    let with_noise = ring.replace(
        "mapping: Front)",
        r#"mapping: Front, breakup: (noise: "grain", scale: 4.0, amount: 0.3))"#,
    );
    load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(&with_noise)),
    )
    .expect("a front with noise resolves");
    // A ragged leading edge (scale-out review 2): a fraction, and only on a front.
    let ragged = with_noise.replace("mapping: Front,", "mapping: Front, edge_noise: 0.5,");
    load(
        dir.path(),
        &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(&ragged)),
    )
    .expect("a ragged front resolves");
    for (bad, why) in [
        (
            with_noise.replace("mapping: Front,", "mapping: Front, edge_noise: 1.0,"),
            "an edge bitten through",
        ),
        (
            with_noise.replace("mapping: Front,", "mapping: Polar, edge_noise: 0.5,"),
            "a ragged edge on a ring that is not a front",
        ),
        (
            with_noise
                .replace("thickness: 0.2", "thickness: 0.5")
                .replace("mapping: Front,", "mapping: Front, edge_noise: 0.5,"),
            "a bite of a quarter of the radius",
        ),
    ] {
        let error = load(
            dir.path(),
            &effect_fixtures::vocabulary_with_noise(&manifest, &noise, "", &splash_effect(&bad)),
        )
        .expect_err(why);
        assert!(
            matches!(
                &error,
                EffectCatalogError::InvalidValue {
                    field: "edge_noise",
                    ..
                }
            ),
            "{why}: {error}"
        );
    }
}

#[test]
fn a_landing_emitter_spawns_only_point_effects() {
    let (dir, manifest) = fixture();
    let emitters = |land: &str| {
        format!(
            r#""globs": (spawn: Burst(count: (base: 4, max: 4)), speed: (1.0, 2.0),
            cone_deg: 30.0, gravity: 10.0, lifetime: (0.5, 1.0), size: (0.05, 0.1),
            texture: "soft_disc", orient: Camera, ramp: "green", land: "{land}")"#
        )
    };
    let fleck = r#""fleck": (class: Minor, event: Point, layers: [
        (block: Decal(texture: "solid", ramp: "blue"), size: (scale: 0.2), duration: 1.0)])"#;
    let thrower = r#"(block: Particles(emitter: "globs"), size: (scale: 0.5))"#;
    let effects = format!("{}, {fleck}", splash_effect(thrower));
    let catalog = load(
        dir.path(),
        &vocabulary(&manifest, &emitters("fleck"), &effects, ""),
    )
    .expect("a Point landing effect resolves");
    assert_eq!(
        catalog.emitters[0].land,
        catalog.effect_id("fleck"),
        "the emitter lands with the fleck"
    );
    let error = load(
        dir.path(),
        &vocabulary(&manifest, &emitters("boom"), &effects, ""),
    )
    .expect_err("a splash effect cannot be a landing");
    assert!(
        matches!(&error, EffectCatalogError::LandEffectNotPoint { effect, .. } if effect == "boom"),
        "{error}"
    );
    let error = load(
        dir.path(),
        &vocabulary(&manifest, &emitters("nowhere"), &effects, ""),
    )
    .expect_err("an unknown landing effect");
    assert!(
        matches!(&error, EffectCatalogError::UnknownName { name, .. } if name == "nowhere"),
        "{error}"
    );
}

/// An effect's random choices are salted by its name, not by its dense id: adding an effect
/// that sorts before it moves its id and keeps its salt, so its picture stays the same.
#[test]
fn an_effects_salt_survives_other_effects_being_added() {
    let (dir, manifest) = fixture();
    let one = load(
        dir.path(),
        &vocabulary(&manifest, "", &splash_effect(SPRITE), ""),
    )
    .expect("one effect");
    let earlier = format!(
        r#""aaa": (class: Major, event: Splash, layers: [ {SPRITE} ]), {}"#,
        splash_effect(SPRITE)
    );
    let two = load(dir.path(), &vocabulary(&manifest, "", &earlier, "")).expect("two effects");
    let (id_one, id_two) = (
        one.effect_id("boom").expect("defined"),
        two.effect_id("boom").expect("defined"),
    );
    assert_ne!(id_one, id_two, "the dense id moves");
    assert_eq!(
        one.effect(id_one).map(|effect| effect.salt),
        two.effect(id_two).map(|effect| effect.salt),
        "the salt does not"
    );
    let pinned =
        format!(r#""boom": (class: Major, event: Splash, seed: Some(2), layers: [ {SPRITE} ])"#);
    let pinned = load(dir.path(), &vocabulary(&manifest, "", &pinned, "")).expect("pinned seed");
    assert_eq!(
        pinned
            .effect_id("boom")
            .and_then(|id| pinned.effect(id))
            .map(|effect| effect.salt),
        Some(2),
        "a pinned seed is the salt"
    );
}
