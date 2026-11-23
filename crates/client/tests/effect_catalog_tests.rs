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
//! are named by no shipped unit; and plan WP11's coverage over the whole catalog: every weapon
//! that publishes a delivery event names the looks of that event, no weapon falls back to a
//! default, and every death has a look; the shot, ride, span and missile fields resolve and
//! refuse their misuse.
//!
//! # Invariants verified
//! - Every failure is typed and names where it is.
//! - Names resolve to dense ids in file order; effect lifetimes cover every layer.

#![allow(clippy::expect_used, clippy::panic, clippy::too_many_lines)]

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

/// WP8 review 2, B2-1: a circle placement loads with a spread in [0, 0.5] and at least one
/// instance; the catalog refuses the rest.
#[test]
fn a_circle_placement_is_range_checked() {
    let (dir, manifest) = fixture();
    let layer = |placement: &str| {
        format!(
            r#"(block: Decal(texture: "solid", ramp: "blue"), size: (scale: 0.2), duration: 1.0,
                placement: {placement})"#
        )
    };
    load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(&layer(
                "Circle(count: 8, radius: (scale: 1.0), spread: 0.1)",
            )),
            "",
        ),
    )
    .expect("a circle loads");
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(&layer(
                "Circle(count: 8, radius: (scale: 1.0), spread: 0.6)",
            )),
            "",
        ),
    )
    .expect_err("a spread above a half");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue {
                field: "spread",
                ..
            }
        ),
        "{error}"
    );
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(&layer("Circle(count: 0, radius: (scale: 1.0))")),
            "",
        ),
    )
    .expect_err("no instances");
    assert!(
        matches!(&error, EffectCatalogError::TooManyInstances { .. }),
        "{error}"
    );
}

/// WP8 review 2, B2-2: a decal or a ground ring may darken the ground; a standing ring or a
/// liquid cannot, and the catalog names the field.
#[test]
fn darkening_needs_a_mark_on_the_ground() {
    let (dir, manifest) = fixture();
    let decal = |extra: &str| {
        format!(
            r#"(block: Decal(texture: "solid", ramp: "blue"{extra}), size: (scale: 1.0), duration: 1.0)"#
        )
    };
    let ring = |extra: &str| {
        format!(
            r#"(block: Ring(texture: "soft_disc", ramp: "green", thickness: 0.2{extra}),
                radius: (from: (scale: 0.5), to: (scale: 1.0)), duration: 1.0)"#
        )
    };
    for (layer, what) in [
        (decal(", darken: true"), "a darkening decal"),
        (
            ring(", orient: Ground, darken: true"),
            "a darkening ground ring",
        ),
    ] {
        let catalog = load(
            dir.path(),
            &vocabulary(&manifest, "", &splash_effect(&layer), ""),
        )
        .expect(what);
        let effect = catalog
            .effect(catalog.effect_id("boom").expect("defined"))
            .expect("loaded");
        let darkens = match &effect.layers[0].block {
            Block::Decal(look) => look.darken,
            Block::Ring { darken, .. } => *darken,
            other => panic!("expected a decal or a ring, got {other:?}"),
        };
        assert!(darkens, "{what} keeps its flag");
    }
    for layer in [
        ring(", darken: true"),
        decal(
            ", darken: true, liquid: (relief: 0.05, sheen: 1.0, gloss: 20.0, value_noise: 0.5, dry_below: 0.5)",
        ),
    ] {
        let error = load(
            dir.path(),
            &vocabulary(&manifest, "", &splash_effect(&layer), ""),
        )
        .expect_err("refused");
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: "darken", .. }),
            "{error}"
        );
    }
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
    load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(&decal(
                ", outline: (edge_noise: 0.2, fill: 0.7, soft: 0.55)",
            )),
            "",
        ),
    )
    .expect("a disc outline with a soft fade loads (WP8 review 3)");
    for (layer, field) in [
        (outline(-0.01, 0.76), "outline.edge_noise"),
        (outline(0.3, 0.76), "outline.edge_noise"),
        (outline(0.02, 0.2), "outline.fill"),
        (outline(0.02, 1.1), "outline.fill"),
        (
            decal(", outline: (edge_noise: 0.02, fill: 0.76, soft: 0.9)"),
            "outline.soft",
        ),
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

/// Plan `2026-11-17-effect-snapshot-identity` WP5 (D-5, B-122): `defaults.status_by_name`
/// keys looks by the spell catalog's status names. They resolve to the catalog's status ids;
/// an unknown status name and a look made for another event kind are each refused with their
/// typed error. The status names are read from the shipped spell catalog, never written here.
#[test]
fn named_status_looks_resolve_against_the_spell_catalog() {
    let (dir, manifest) = fixture();
    let units = sim_fixtures::shipped_catalogs().units;
    let spells = units.spells();
    let names: Vec<(sim_types::StatusId, String)> = (0..spells.status_count())
        .filter_map(|index| {
            let id = sim_types::StatusId(u16::try_from(index).ok()?);
            Some((id, spells.status(id)?.spec.name.clone()))
        })
        .collect();
    let [(first, first_name), (second, second_name), ..] = names.as_slice() else {
        panic!("the shipped spell catalog has at least two statuses");
    };
    let shell = r"(block: Shell(rim: (1.0, 0.3, 0.2, 0.6)), duration: 0.3, held: true)";
    let point = SPRITE.replace("bind: SplashReach, ", "");
    let effects = format!(
        r#""look": (class: Minor, event: Status, layers: [ {shell} ]),
           "hit": (class: Minor, event: Point, layers: [ {point} ])"#
    );
    let with_names = |defaults: &str| {
        load(dir.path(), &vocabulary(&manifest, "", &effects, defaults))
            .expect("the vocabulary resolves its effect names")
    };
    let catalog = with_names(&format!(r#"status_by_name: {{ "{first_name}": "look" }}"#));
    let look = catalog.effect_id("look").expect("defined");
    assert_eq!(
        catalog.defaults.status_by_name.get(first_name),
        Some(&look),
        "the vocabulary resolves the effect name"
    );
    let (units, _) = units_with_block(ArchetypeEffects::default());
    let tables = UnitEffectTables::build(&units, &catalog).expect("the status name resolves");
    assert_eq!(tables.named_status(*first), Some(look));
    assert_eq!(tables.named_status(*second), None, "only the named status");

    // An unknown effect name is the vocabulary's error.
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &effects,
            &format!(r#"status_by_name: {{ "{first_name}": "no_such_look" }}"#),
        ),
    )
    .expect_err("unknown effect");
    assert!(
        matches!(&error, EffectCatalogError::UnknownName { name, kind: NameKind::Effect, .. }
            if name == "no_such_look"),
        "{error}"
    );
    // An unknown status name.
    let catalog = with_names(r#"status_by_name: { "no_such_status": "look" }"#);
    let error = UnitEffectTables::build(&units, &catalog).expect_err("unknown status");
    assert!(
        matches!(&error, EffectCatalogError::UnknownStatus { name, .. } if name == "no_such_status"),
        "{error}"
    );
    assert_names(&error, &["vocabulary.ron", "no_such_status"]);
    // A look made for another event kind.
    let catalog = with_names(&format!(r#"status_by_name: {{ "{second_name}": "hit" }}"#));
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

/// The shipped `defaults.status_by_name` resolves against the shipped spell catalog, and each
/// named look is a `Status` effect that holds a `Critical` layer (plan
/// `2026-11-17-effect-snapshot-identity` WP5).
#[test]
fn shipped_named_status_looks_hold_a_critical_layer() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = UnitEffectTables::build(&units, &catalog).expect("the shipped blocks resolve");
    assert!(
        !catalog.defaults.status_by_name.is_empty(),
        "the vocabulary names a status look (B-122)"
    );
    for (name, look) in &catalog.defaults.status_by_name {
        let status = units.spells().status_id(name).expect("a catalog status");
        assert_eq!(tables.named_status(status), Some(*look), "{name}");
        let effect = catalog.effect(*look).expect("loaded");
        assert_eq!(effect.event, EventKind::Status, "{name}");
        assert!(
            effect.layers.iter().any(|layer| layer.held
                && layer.priority == client::effects::catalog::schema::Priority::Critical),
            "{name}: {} holds a Critical layer",
            effect.name
        );
    }
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

/// The splash effects the review gates accepted (the slice in round 5, plan Amendment 3; the
/// scale-out in rounds 1 to 5, Amendment 9): the unit that names each, the weapon slot, the mode
/// whose weapon override carries it (`None` for the default stance) and the effect. Test data restating the shipped catalog, so a block dropped from
/// `units.ron` fails here.
const ACCEPTED_SLICE: [(&str, WeaponSlot, Option<&str>, &str); 7] = [
    (
        "railgun_crawler",
        WeaponSlot::Primary,
        Some("siege_mode"),
        "shell_impact",
    ),
    ("volatile_tick", WeaponSlot::Primary, None, "acid_burst"),
    (
        "burrow_scorpion",
        WeaponSlot::Primary,
        Some("burrowed"),
        "spine_line",
    ),
    ("sky_bastion", WeaponSlot::Air, None, "flak_burst"),
    ("napalm_wheeler", WeaponSlot::Primary, None, "flame_line"),
    (
        "tripod_annihilator",
        WeaponSlot::Primary,
        None,
        "lance_sweep",
    ),
    ("stag_behemoth", WeaponSlot::Primary, None, "cleave_impact"),
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
    for (slug, slot, mode, effect) in ACCEPTED_SLICE {
        let archetype = units
            .id_by_slug(slug)
            .and_then(|id| units.get(id).ok())
            .unwrap_or_else(|| panic!("{slug} is a shipped unit"));
        let named = archetype.effects.weapons.iter().any(|entry| {
            entry.weapon == slot
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

/// Every shipped torn front (a `Critical` `Front` ring with `edge_noise`) keeps dust on the reach
/// in every footprint sector, and its consecutive hits place their crests differently, for 64
/// consecutive hits drawn with the effect's real salt and the ring's real layer index through
/// the runtime's own seed functions (plan Amendment 8; review 5: one GPU capture checks one
/// seed, this checks the construction for many).
#[test]
fn shipped_torn_fronts_hold_every_sector_for_many_hits() {
    use client::effects::catalog::schema::Priority;
    use client::effects::front::{crest_centres, turn_distance, CREST_HALF_WIDTH_DEG};
    use client::effects::runtime::{instance_seed, placed_seed};
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let mut fronts = 0;
    for effect in &catalog.effects {
        for (layer_index, layer) in effect.layers.iter().enumerate() {
            let Block::Ring { edge_noise, .. } = layer.block else {
                continue;
            };
            if edge_noise <= 0.0 || layer.priority != Priority::Critical {
                continue;
            }
            fronts += 1;
            let layer_index = u32::try_from(layer_index).expect("a small index");
            let seeds: Vec<u32> = (0..64_u32)
                .map(|hit| placed_seed(instance_seed(hit, effect.salt), layer_index, 0, 0))
                .collect();
            for seed in &seeds {
                let centres = crest_centres(*seed);
                for sector in 0..8_u16 {
                    let on_crest = (0..9_u16).any(|ray| {
                        let angle = f32::from(sector * 9 + ray) / 72.0;
                        centres.iter().any(|centre| {
                            turn_distance(angle, *centre) * 360.0 <= CREST_HALF_WIDTH_DEG + 1e-3
                        })
                    });
                    assert!(on_crest, "{}: seed {seed:#x}, sector {sector}", effect.name);
                }
            }
            let mut total = 0.0;
            for pair in seeds.windows(2) {
                let (a, b) = (crest_centres(pair[0]), crest_centres(pair[1]));
                let mean = a
                    .iter()
                    .map(|centre| {
                        b.iter()
                            .map(|other| turn_distance(*centre, *other))
                            .fold(f32::MAX, f32::min)
                    })
                    .sum::<f32>()
                    * 360.0
                    / 12.0;
                assert!(
                    mean >= 1.0,
                    "{}: consecutive hits share their crests",
                    effect.name
                );
                total += mean;
            }
            assert!(
                total / 63.0 >= 5.0,
                "{}: hits differ too little",
                effect.name
            );
        }
    }
    assert!(fronts > 0, "the shipped vocabulary has a torn front");
}

/// Plan WP9: a `Status` effect, like a `Mode` effect, may hold layers and shape its unit's
/// bubble (with a pulse of its tint and rim) and may lay or spawn other `Status` effects; any
/// other kind may not, and every new value is range-checked, naming its field.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one fixture per new rule, read top to bottom"
)]
fn status_effects_hold_layers_lay_and_spawn_and_their_values_are_checked() {
    let (dir, manifest) = fixture();
    let shell = r"(block: Shell(rim: (1.0, 0.3, 0.2, 0.6), pulse: Some((period: 0.6, depth: 0.5))),
        duration: 0.3, held: true)";
    let card = r#"(block: Sprite(texture: "soft_disc", ramp: "white", shape: Membrane),
        size: (bind: UnitRadius, scale: 1.2), duration: 0.3, held: true,
        strength: Some((bind: BarrierRatio, floor: 0.4)))"#;
    let status = |extra: &str, layers: &str| {
        format!(
            r#""look": (class: Minor, event: Status{extra}, layers: [ {layers} ]),
               "puff": (class: Ambient, event: Status, layers: [ {card} ])"#
        )
    };
    let catalog = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &status(
                r#", emit: Some((effect: "puff", every: 0.1, moving: true)),
                     hit: Some((effect: "puff", every: 0.2))"#,
                &format!("{shell}, {card}"),
            ),
            r#"status: { Barrier: "look" }"#,
        ),
    )
    .expect("a status look that holds, lays and spawns loads");
    let look_id = catalog.effect_id("look").expect("defined");
    let look = catalog.effect(look_id).expect("loaded");
    let puff = catalog.effect_id("puff").expect("defined");
    let emit = look.emit.expect("it lays an effect");
    assert_eq!((emit.effect, emit.every, emit.moving), (puff, 0.1, true));
    let hit = look.hit.expect("it spawns on a hit");
    assert_eq!((hit.effect, hit.every), (puff, 0.2));
    let Block::Shell(shell_look) = &look.layers[0].block else {
        panic!("the first layer shapes the bubble");
    };
    assert!(
        (shell_look.beat(0.0) - 1.0).abs() < 1e-6,
        "a beat peaks at its start"
    );
    assert!(
        (shell_look.beat(0.3) - 0.5).abs() < 1e-5,
        "half a period later it is down by its depth"
    );
    assert!(
        look.layers[1].strength.is_some(),
        "the membrane's strength is bound"
    );
    assert_eq!(
        catalog.defaults.status.get(&sim_types::StatusKind::Barrier),
        Some(&look_id)
    );

    // A splash effect may not hold, shape a bubble, lay or spawn.
    for (effect, field) in [
        (
            format!(r#""boom": (class: Major, event: Splash, layers: [ {shell} ])"#),
            "held",
        ),
        (
            format!(
                r#""boom": (class: Major, event: Splash, emit: Some((effect: "boom", every: 0.1)),
                    layers: [ {SPRITE} ])"#
            ),
            "emit",
        ),
        (
            format!(
                r#""boom": (class: Major, event: Splash, hit: Some((effect: "boom", every: 0.1)),
                    layers: [ {SPRITE} ])"#
            ),
            "hit",
        ),
    ] {
        let error = load(dir.path(), &vocabulary(&manifest, "", &effect, "")).expect_err(field);
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: found, .. } if *found == field),
            "{field}: {error}"
        );
    }
    // What a status lays or spawns must be a status effect itself.
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &format!(
                r#""look": (class: Minor, event: Status, hit: Some((effect: "boom", every: 0.1)),
                    layers: [ {shell} ]), {}"#,
                splash_effect(SPRITE)
            ),
            "",
        ),
    )
    .expect_err("a hit naming a splash effect");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue { field: "hit", .. }
        ),
        "{error}"
    );
    // Out-of-range values name their field.
    let shell_with = |pulse: &str| {
        format!(r"(block: Shell(rim: (1.0, 0.3, 0.2, 0.6), pulse: Some({pulse})), duration: 0.3)")
    };
    let card_with = |strength: &str| {
        format!(
            r#"(block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 1.0),
                duration: 0.3, strength: Some({strength}))"#
        )
    };
    for (effects, field) in [
        (
            status("", &shell_with("(period: 0.0, depth: 0.5)")),
            "pulse.period",
        ),
        (
            status("", &shell_with("(period: 0.6, depth: 1.5)")),
            "pulse.depth",
        ),
        (
            status("", &card_with("(bind: BarrierRatio, floor: 1.2)")),
            "strength.floor",
        ),
        (
            status("", &card_with("(bind: BarrierRatio, span: 0.0)")),
            "strength.span",
        ),
        (
            status("", &card_with("(bind: BarrierRatio, span: -0.5)")),
            "strength.span",
        ),
        (
            status("", &card_with("(bind: BarrierRatio, span: inf)")),
            "strength.span",
        ),
        (
            status(r#", emit: Some((effect: "puff", every: 0.0))"#, shell),
            "emit.every",
        ),
        (
            status(r#", hit: Some((effect: "puff", every: -1.0))"#, shell),
            "hit.every",
        ),
    ] {
        let error = load(dir.path(), &vocabulary(&manifest, "", &effects, "")).expect_err(field);
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: found, .. } if *found == field),
            "{field}: {error}"
        );
    }
    // A splash effect does not publish the barrier's fraction.
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(&card_with("(bind: BarrierRatio, floor: 0.2)")),
            "",
        ),
    )
    .expect_err("an unbound strength");
    assert!(
        matches!(&error, EffectCatalogError::UnboundBinding { .. }),
        "{error}"
    );
    // A membrane is the outline of a body: a ribbon cannot draw it.
    let ribbon = r#"(block: Ribbon(texture: "soft_disc", ramp: "white", shape: Membrane),
        size: (scale: 0.2), duration: 0.5)"#;
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &format!(r#""jet": (class: Major, event: Line, layers: [ {ribbon} ])"#),
            "",
        ),
    )
    .expect_err("a membrane ribbon");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue { field: "shape", .. }
        ),
        "{error}"
    );
}

/// Plan WP9: the shipped vocabulary shows the four statuses by default, each a `Status` effect
/// whose look is held and `Critical`; it names no default death (the glass shatter stays the
/// default), and every death a shipped block names is a `Death` effect that replaces the
/// shatter for that unit only.
#[test]
fn shipped_statuses_have_held_looks_and_named_deaths_replace_the_shatter() {
    use sim_types::StatusKind;
    let root = ResolvedAssetsRoot::resolve().expect("assets root");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    for kind in [
        StatusKind::Overclock,
        StatusKind::Slowed,
        StatusKind::Dash,
        StatusKind::Barrier,
    ] {
        let id = catalog
            .defaults
            .status
            .get(&kind)
            .copied()
            .expect("a default look");
        let effect = catalog.effect(id).expect("loaded");
        assert_eq!(effect.event, EventKind::Status, "{kind:?}");
        assert!(
            effect.layers.iter().any(|layer| layer.held
                && layer.priority == client::effects::catalog::schema::Priority::Critical),
            "{kind:?}: {} holds a Critical layer",
            effect.name
        );
    }
    // Plan WP11 (Amendment 24), re-baselined with that cause only: the vocabulary's default
    // death, `glass_shatter`, replaced the legacy glass shatter, so every unit has a death look:
    // its block's own, or the default.
    let default_death = catalog
        .defaults
        .death
        .expect("a default death replaces the legacy shatter");
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = UnitEffectTables::build(&units, &catalog).expect("the shipped blocks resolve");
    let mut named = 0;
    for arch in units.archetypes() {
        let death = tables.death(arch.unit_type_id);
        let own = arch
            .effects
            .death
            .as_ref()
            .and_then(|name| catalog.effect_id(name));
        assert_eq!(
            death,
            Some(own.unwrap_or(default_death)),
            "{}: its block's death effect, or the default",
            arch.slug
        );
        assert_eq!(
            client::effects::systems::replaces_shatter(Some(&tables), arch.unit_type_id),
            death.is_some(),
            "{}",
            arch.slug
        );
        if let Some(id) = death {
            named += 1;
            assert_eq!(catalog.effect(id).expect("loaded").event, EventKind::Death);
        }
    }
    assert!(named > 0, "some shipped unit names its death");
    assert!(!client::effects::systems::replaces_shatter(
        None,
        sim_types::UnitTypeId(0)
    ));
}

/// Plan `2026-11-05-sc2-deviation-fixes` `WP13d`: a researched Concussive Shells hit of the
/// shipped Marauder analogue publishes the slow on the struck unit's bubble, which the client
/// shows with the shipped default `Slowed` look, `concussive_daze`; without the research the hit
/// publishes nothing.
#[test]
fn a_researched_concussive_hit_publishes_the_slow_the_daze_look_shows() {
    use client::effects::stance::status_shown;
    use sim_types::{PlayerId, SimScalar, SimVec2, StatusKind, Tick, TickBundle, UnitOrder};
    let root = ResolvedAssetsRoot::resolve().expect("assets root");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let look = catalog
        .defaults
        .status
        .get(&StatusKind::Slowed)
        .copied()
        .expect("a default slowed look");
    assert_eq!(
        catalog.effect(look).expect("loaded").name,
        "concussive_daze"
    );

    for researched in [false, true] {
        let map = sim_map::GameMap::new(
            48,
            48,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )
        .expect("a flat map");
        let mut world = sim_fixtures::world(map, 0x0D_A2E);
        if researched {
            world
                .research_upgrade(PlayerId(0), "concussive_shells")
                .expect("research admitted");
        }
        let spawn = |world: &mut sim_core::SimWorld, slug: &str, owner: u8, x: i32| {
            let kind = world.catalog.id_by_slug(slug).expect("a shipped unit");
            let at = SimVec2::new(SimScalar::from_i32(x), SimScalar::from_i32(20));
            let id = world
                .spawn_unit(kind, PlayerId(owner), at)
                .expect("open ground");
            world
                .entities
                .get_mut(&id)
                .expect("spawned")
                .issue_order(UnitOrder::HoldPosition, false)
                .expect("hold admitted");
            id
        };
        let golem = spawn(&mut world, "shock_golem", 0, 20);
        let target = spawn(&mut world, "pulse_bot", 1, 24);
        world
            .entities
            .get_mut(&golem)
            .expect("spawned")
            .issue_order(
                UnitOrder::AttackTarget {
                    target,
                    origin: sim_types::AttackOrigin::default(),
                },
                false,
            )
            .expect("attack admitted");
        let life = world.entities[&target].hp;
        let snapshot = (0..120)
            .find_map(|_| {
                let snapshot = world.step(&TickBundle {
                    tick: Tick(world.current_tick),
                    commands: Vec::new(),
                    state_checksum: None,
                });
                (world.entities[&target].hp < life).then_some(snapshot)
            })
            .expect("the golem hits within 120 ticks");
        let bubble = snapshot
            .bubbles
            .iter()
            .find(|bubble| bubble.entity_id == target)
            .expect("the target is drawn");
        assert_eq!(
            status_shown(bubble, StatusKind::Slowed),
            researched,
            "researched: {researched}"
        );
    }
}

/// Plan WP10: `Area` and `Link` effects may hold layers (no `Shell`), an area may lay another
/// `Area` effect (never "while moving"), a ribbon spans a tether's line, and a block's
/// `abilities` resolve by the archetype's ability name into a table keyed by ability index.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "one fixture per new rule, read top to bottom"
)]
fn area_and_link_looks_hold_and_are_named_by_their_abilities() {
    let (dir, manifest) = fixture();
    let disc = r#"(block: Decal(texture: "soft_disc", ramp: "white"),
        size: (bind: AreaRadius, scale: 1.0), duration: 0.2, held: true, priority: Critical)"#;
    let puff = r#"(block: Sprite(texture: "soft_disc", ramp: "white"),
        size: (bind: AreaRadius, scale: 0.3), duration: 0.4)"#;
    let rope = r#"(block: Ribbon(texture: "soft_disc", ramp: "white"), size: (scale: 0.05),
        duration: 0.1, held: true, priority: Critical)"#;
    let effects = |area_extra: &str, link_layers: &str| {
        format!(
            r#""cloud": (class: Major, event: Area{area_extra}, layers: [ {disc} ]),
               "flare": (class: Minor, event: Area, layers: [ {puff} ]),
               "rope": (class: Minor, event: Link, layers: [ {link_layers} ])"#
        )
    };
    let catalog = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &effects(r#", emit: Some((effect: "flare", every: 0.1))"#, rope),
            "",
        ),
    )
    .expect("held areas and tethers load");
    let cloud = catalog.effect_id("cloud").expect("defined");
    let emit = catalog
        .effect(cloud)
        .and_then(|effect| effect.emit)
        .expect("it lays");
    assert_eq!(Some(emit.effect), catalog.effect_id("flare"));

    let refused = [
        (
            effects(
                r#", emit: Some((effect: "flare", every: 0.1, moving: true))"#,
                rope,
            ),
            "emit.moving",
        ),
        (
            effects(r#", emit: Some((effect: "rope", every: 0.1))"#, rope),
            "emit",
        ),
        (
            effects(r#", hit: Some((effect: "flare", every: 0.1))"#, rope),
            "hit",
        ),
        (
            effects(
                "",
                r"(block: Shell(rim: (1.0, 0.3, 0.2, 0.6)), duration: 0.3, held: true)",
            ),
            "held",
        ),
    ];
    for (text, field) in refused {
        let error = load(dir.path(), &vocabulary(&manifest, "", &text, "")).expect_err(field);
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: found, .. } if *found == field),
            "{field}: {error}"
        );
    }
    let held_splash = splash_effect(&SPRITE.replace("priority: Critical", "held: true"));
    let error = load(dir.path(), &vocabulary(&manifest, "", &held_splash, ""))
        .expect_err("a splash holds nothing");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue { field: "held", .. }
        ),
        "{error}"
    );

    // The block's abilities, by name, against the first archetype with two abilities.
    let mut units = sim_fixtures::shipped_catalogs().units;
    let ids: Vec<_> = units.archetypes().map(|arch| arch.unit_type_id).collect();
    for id in &ids {
        units.get_mut(*id).expect("the archetype exists").effects = ArchetypeEffects::default();
    }
    let caster = units
        .archetypes()
        .find(|arch| arch.abilities.len() >= 2)
        .map(|arch| arch.unit_type_id)
        .expect("a shipped unit has two abilities");
    let names: Vec<String> = units
        .get(caster)
        .expect("the archetype exists")
        .abilities
        .iter()
        .map(|ability| ability.name.clone())
        .collect();
    let block = |pairs: &[(&str, &str)]| ArchetypeEffects {
        abilities: pairs
            .iter()
            .map(|(ability, look)| ((*ability).to_owned(), (*look).to_owned()))
            .collect(),
        ..ArchetypeEffects::default()
    };
    units.get_mut(caster).expect("the archetype exists").effects =
        block(&[(&names[1], "cloud"), (&names[0], "rope")]);
    let tables = UnitEffectTables::build(&units, &catalog).expect("the abilities resolve");
    let source = |ability: u8| sim_types::AbilitySource {
        unit_type: caster,
        ability,
    };
    assert_eq!(tables.ability(source(1)), Some(cloud));
    assert_eq!(tables.ability(source(0)), catalog.effect_id("rope"));
    assert_eq!(tables.ability(source(5)), None, "no look, no default");

    units.get_mut(caster).expect("the archetype exists").effects =
        block(&[("No Such Ability", "cloud")]);
    let error = UnitEffectTables::build(&units, &catalog).expect_err("unknown ability");
    assert!(
        matches!(
            &error,
            EffectCatalogError::UnknownUnitAbility { ability, .. } if ability == "No Such Ability"
        ),
        "{error}"
    );
    let point = format!(
        r#"{}, "hit": (class: Minor, event: Point, layers: [ {} ])"#,
        effects("", rope),
        SPRITE.replace("bind: SplashReach, ", "")
    );
    let with_point = load(dir.path(), &vocabulary(&manifest, "", &point, "")).expect("loads");
    units.get_mut(caster).expect("the archetype exists").effects = block(&[(&names[0], "hit")]);
    let error = UnitEffectTables::build(&units, &with_point)
        .expect_err("an ability's look is an area or a tether");
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

/// Plan WP10: every shipped ability look is an `Area` or `Link` effect holding a `Critical`
/// layer and no light, and every area look's `Critical` layers are bound to `AreaRadius`. Plan
/// WP11 (F1): a look may also be a `Missile` effect, which holds a `Critical` layer and lays an
/// `Area` impact whose `Critical` layers bind `AreaRadius`, neither with a light.
#[test]
fn shipped_ability_looks_hold_their_critical_extent() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    let mut named = 0;
    for arch in units.archetypes() {
        for (ability, look) in &arch.effects.abilities {
            assert!(
                arch.abilities.iter().any(|spec| spec.name == *ability),
                "{} names its own ability {ability}",
                arch.slug
            );
            let effect = catalog
                .effect_id(look)
                .and_then(|id| catalog.effect(id))
                .expect("the look is defined");
            assert!(
                matches!(
                    effect.event,
                    EventKind::Area | EventKind::Link | EventKind::Missile
                ),
                "{look}"
            );
            if effect.event == EventKind::Missile {
                let impact = effect
                    .impact
                    .and_then(|id| catalog.effect(id))
                    .unwrap_or_else(|| panic!("{look} lays an impact"));
                assert_eq!(impact.event, EventKind::Area, "{look}'s impact");
                let critical: Vec<_> = impact
                    .layers
                    .iter()
                    .filter(|layer| {
                        layer.priority == client::effects::catalog::schema::Priority::Critical
                    })
                    .collect();
                assert!(!critical.is_empty(), "{look}'s impact marks its extent");
                assert!(
                    critical.iter().all(|layer| {
                        layer.size.bind == Binding::AreaRadius
                            || layer
                                .radius
                                .is_some_and(|radius| radius.to.bind == Binding::AreaRadius)
                    }),
                    "{look}'s impact's Critical layers bind AreaRadius"
                );
                assert!(
                    impact
                        .layers
                        .iter()
                        .all(|layer| !matches!(layer.block, Block::Light { .. })),
                    "{look}'s impact lights nothing"
                );
            }
            let critical: Vec<_> = effect
                .layers
                .iter()
                .filter(|layer| {
                    layer.held
                        && layer.priority == client::effects::catalog::schema::Priority::Critical
                })
                .collect();
            assert!(!critical.is_empty(), "{look} holds a Critical layer");
            // A point light is not cut where the fog begins, so it would show an enemy's area
            // to a viewer who does not see it (D-5).
            assert!(
                effect
                    .layers
                    .iter()
                    .all(|layer| !matches!(layer.block, Block::Light { .. })),
                "{look} lights nothing"
            );
            if effect.event == EventKind::Area {
                assert!(
                    critical.iter().all(|layer| {
                        layer.size.bind == Binding::AreaRadius
                            || layer
                                .radius
                                .is_some_and(|radius| radius.to.bind == Binding::AreaRadius)
                    }),
                    "{look}'s Critical layers bind AreaRadius"
                );
            }
            named += 1;
        }
    }
    assert!(
        named >= 3,
        "the storm, the cloud and the tether are named: {named}"
    );
}

/// What a weapon's attack publishes for the client to draw (plan WP11), as `sim_core`'s
/// combat decides it: a weapon with a projectile speed launches a projectile; an instant weapon
/// publishes the event of its visual kind; an instant weapon whose visual kind is `Projectile`
/// or `None` publishes no delivery event at all (its splash, if any, is all it shows).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Delivery {
    /// A `ProjectileVisual`: a flight from the muzzle to the target.
    Projectile,
    /// A `HitscanTracer` or a `BeamVisual`: an instant line from the source to the impact.
    Line,
    /// A `MeleeSlashVisual` at the contact point.
    Melee,
    /// Nothing but its splash.
    Nothing,
}

/// The delivery of `weapon`.
fn delivery(weapon: &sim_types::WeaponSpec) -> Delivery {
    use sim_types::WeaponVisualKind as Kind;
    if weapon.projectile_speed.is_some() {
        return Delivery::Projectile;
    }
    match weapon.visual_kind {
        Kind::Hitscan | Kind::LaserBeam | Kind::LightningArc => Delivery::Line,
        Kind::MeleeSlash => Delivery::Melee,
        Kind::Projectile | Kind::None => Delivery::Nothing,
    }
}

/// Every weapon of `archetype` with the key the simulation tags its events with
/// (`sim_core::world::presentation::weapon_source`): the air weapon is `Air`; the ground weapon
/// and every mode's override take `Ground` on a structure or on a unit whose ground weapon
/// differs from its weapon, and `Primary` otherwise; an override names its mode.
fn tagged_weapons(
    archetype: &sim_core::catalog::UnitArchetype,
) -> Vec<(WeaponSlot, Option<String>, &sim_types::WeaponSpec)> {
    let structure = matches!(archetype.shape, sim_types::ShapeKind::BuildingDome { .. });
    let ground_slot = if structure || archetype.ground_weapon != archetype.weapon {
        WeaponSlot::Ground
    } else {
        WeaponSlot::Primary
    };
    let mut weapons = Vec::new();
    if let Some(ground) = archetype
        .ground_weapon
        .as_ref()
        .or(archetype.weapon.as_ref())
    {
        weapons.push((ground_slot, None, ground));
    }
    if let Some(air) = archetype.air_weapon.as_ref() {
        weapons.push((WeaponSlot::Air, None, air));
    }
    for mode in &archetype.modes {
        if let Some(Some(spec)) = mode.ground_weapon_override.as_ref() {
            weapons.push((ground_slot, Some(mode.mode_name.clone()), spec));
        }
    }
    weapons
}

/// Whether `effect` flies a projectile's path: a `Critical` card riding its line or a
/// `Critical` strip riding it, lasting the flight.
fn flies(effect: &client::effects::catalog::Effect) -> bool {
    effect.layers.iter().any(|layer| {
        let rides = match &layer.block {
            Block::Sprite(look) => look.ride,
            Block::Ribbon(look) => look.trail.is_some(),
            _ => false,
        };
        layer.priority == client::effects::catalog::schema::Priority::Critical
            && rides
            && layer.lasts == Some(Binding::FlightTime)
    })
}

/// Plan WP11's coverage rule over the whole catalog: every weapon of every shipped unit and
/// structure that publishes a delivery event is named in its block, so the effect system draws
/// it and no legacy card does; the names are of the right kind for the event (a projectile's
/// travel flies its path for the flight, an instant line's travel spans it at once, a melee
/// strike has an impact); no weapon falls back to a default (the vocabulary has no weapon
/// default at all), and every death has a look (the default one is the glass breaking).
#[test]
fn every_shipped_weapon_names_the_looks_of_the_event_it_publishes() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    assert_eq!(catalog.defaults.splash, None, "no default splash effect");
    assert_eq!(catalog.defaults.impact, None, "no default impact effect");
    let death = catalog
        .defaults
        .death
        .and_then(|id| catalog.effect(id))
        .expect("a default death replaces the legacy shatter");
    assert_eq!(death.event, EventKind::Death);
    let units = sim_fixtures::shipped_catalogs().units;
    let tables = UnitEffectTables::build(&units, &catalog).expect("every block resolves");
    let effect = |name: &Option<String>| {
        name.as_ref().map(|name| {
            catalog
                .effect_id(name)
                .and_then(|id| catalog.effect(id))
                .unwrap_or_else(|| panic!("{name} resolves"))
        })
    };
    let mut report = Vec::new();
    let mut silent = Vec::new();
    for archetype in units.archetypes() {
        assert!(
            tables.death(archetype.unit_type_id).is_some(),
            "{} has a death look",
            archetype.slug
        );
        for (slot, mode, spec) in tagged_weapons(archetype) {
            let label = format!(
                "{} {slot:?} {mode:?} {:?}",
                archetype.slug, spec.visual_kind
            );
            let delivery = delivery(spec);
            let entry = archetype
                .effects
                .weapons
                .iter()
                .find(|entry| entry.weapon == slot && entry.mode == mode);
            if delivery == Delivery::Nothing && spec.splash.is_none() {
                assert!(
                    entry.is_none_or(|entry| entry.travel.is_none() && entry.impact.is_none()),
                    "{label}: publishes no delivery event, so it names no travel or impact"
                );
                silent.push(label);
                continue;
            }
            let entry = entry.unwrap_or_else(|| panic!("{label}: the block names no looks"));
            let travel = effect(&entry.travel);
            let impact = effect(&entry.impact);
            let splash = effect(&entry.splash);
            assert_eq!(
                spec.splash.is_some(),
                splash.is_some(),
                "{label}: a splash weapon, and only one, names its splash"
            );
            // Plan `2026-11-23-splash-centres` D-8 item 2: an area that opens only below a
            // target radius publishes nothing at a larger target, so the shot names an impact
            // and still shows a landing there.
            if spec
                .splash
                .as_ref()
                .is_some_and(|splash| splash.target_radius_below.is_some())
            {
                assert!(
                    impact.is_some(),
                    "{label}: a conditional area names an impact for the shots it does not open on"
                );
            }
            match delivery {
                Delivery::Projectile => {
                    let travel = travel.unwrap_or_else(|| panic!("{label}: no travel"));
                    assert!(
                        flies(travel),
                        "{label}: {} does not fly the path",
                        travel.name
                    );
                    assert!(impact.is_some() || splash.is_some(), "{label}: no impact");
                }
                Delivery::Line => {
                    if let Some(travel) = travel {
                        assert!(
                            !travel.layers.iter().any(|layer| layer.lasts.is_some()),
                            "{label}: {} waits for a flight an instant shot has not",
                            travel.name
                        );
                    } else {
                        assert!(
                            splash.is_some(),
                            "{label}: an instant line without a travel must show its splash"
                        );
                    }
                    assert!(impact.is_some() || splash.is_some(), "{label}: no impact");
                }
                Delivery::Melee => {
                    assert!(travel.is_none(), "{label}: a melee strike has no travel");
                    assert!(impact.is_some() || splash.is_some(), "{label}: no impact");
                }
                Delivery::Nothing => {
                    assert!(travel.is_none() && impact.is_none(), "{label}: no event");
                }
            }
            if let Some(travel) = travel {
                assert_eq!(travel.event, EventKind::Shot, "{label}: travel kind");
                // Plan `2026-11-17-effect-snapshot-identity` D-4: only a beam is held while its
                // attacker keeps striking; its strikes are instant lines published as beams.
                if travel.sustain.is_some() {
                    assert!(
                        delivery == Delivery::Line
                            && matches!(
                                spec.visual_kind,
                                sim_types::WeaponVisualKind::LaserBeam
                                    | sim_types::WeaponVisualKind::LightningArc
                            ),
                        "{label}: {} is sustained, but the weapon publishes no instant beam",
                        travel.name
                    );
                }
            }
            if let Some(impact) = impact {
                assert_eq!(impact.event, EventKind::Point, "{label}: impact kind");
            }
            let source = EffectSource {
                unit_type: archetype.unit_type_id,
                weapon: WeaponKey {
                    slot,
                    mode: mode.as_deref().and_then(|name| {
                        archetype
                            .modes
                            .iter()
                            .position(|candidate| candidate.mode_name == name)
                            .and_then(|index| u8::try_from(index).ok())
                    }),
                },
            };
            assert!(
                tables.names_weapon(source),
                "{label}: the tables find its entry by the key the simulation tags it with"
            );
            report.push(format!(
                "{label} -> travel {:?}, impact {:?}, splash {:?}",
                entry.travel, entry.impact, entry.splash
            ));
        }
    }
    eprintln!(
        "weapon coverage: {} weapons named\n  {}\nweapons that publish no delivery event: {}\n  {}",
        report.len(),
        report.join("\n  "),
        silent.len(),
        silent.join("\n  ")
    );
    // B-123 (plan `2026-11-17-effect-snapshot-identity` WP3): every shipped weapon publishes a
    // delivery or splash event, so none is silent.
    assert_eq!(
        silent,
        Vec::<String>::new(),
        "weapons that publish no event"
    );
}

/// Plan `2026-11-17-effect-snapshot-identity` WP7 (D-4): a `Shot` effect may be sustained
/// (held while its attacker keeps striking) and then hold layers that erode out within 0.1 s;
/// `sustain` is refused on any other kind of effect, with a grace that is not a positive number
/// of seconds, and on a look whose held layers take longer to go; a `Shot` that is not
/// sustained may hold no layer; and every shipped sustained look is held by beams only.
#[test]
fn a_sustained_shot_holds_layers_that_go_within_a_tenth_of_a_second() {
    let (dir, manifest) = fixture();
    let ribbon = |extra: &str| {
        format!(
            r#"(block: Ribbon(texture: "solid", ramp: "white", shape: Radial),
                size: (scale: 0.1), duration: 0.05{extra}, priority: Critical)"#
        )
    };
    let shot = |sustain: &str, layer: &str| {
        format!(r#""beam": (class: Minor, event: Shot{sustain}, layers: [ {layer} ])"#)
    };
    let held = ribbon(", held: true, exit: 0.08");
    let catalog = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &shot(", sustain: Some((grace: 0.12))", &held),
            "",
        ),
    )
    .expect("a sustained beam whose held layers go within 0.1 s resolves");
    let beam = catalog
        .effect(catalog.effect_id("beam").expect("defined"))
        .expect("loaded");
    assert_eq!(beam.sustain.map(|sustain| sustain.grace), Some(0.12));
    assert!(beam.layers[0].held && beam.held_exit <= 0.1);

    let refused = |effects: String, field: &str| {
        let error =
            load(dir.path(), &vocabulary(&manifest, "", &effects, "")).expect_err("refused");
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: found, .. } if *found == field),
            "{field}: {error}"
        );
        assert_names(&error, &["vocabulary.ron", "beam"]);
    };
    // Sustain on another kind of effect.
    refused(
        format!(
            r#""beam": (class: Minor, event: Point, sustain: Some((grace: 0.12)), layers: [ {} ])"#,
            SPRITE.replace("bind: SplashReach, ", "")
        ),
        "sustain",
    );
    // A grace that is not positive.
    refused(shot(", sustain: Some((grace: 0.0))", &held), "grace");
    // Held layers that take longer than 0.1 s to go.
    refused(
        shot(
            ", sustain: Some((grace: 0.12))",
            &ribbon(", held: true, exit: 0.2"),
        ),
        "exit",
    );
    // A shot that is not sustained holds nothing.
    refused(shot("", &held), "held");

    // Every shipped sustained look is a held beam's.
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let shipped = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let sustained: Vec<&str> = shipped
        .effects
        .iter()
        .filter(|effect| effect.sustain.is_some())
        .map(|effect| effect.name.as_str())
        .collect();
    assert!(
        !sustained.is_empty(),
        "the shipped beams are sustained (B-126)"
    );
    for effect in shipped
        .effects
        .iter()
        .filter(|effect| effect.sustain.is_some())
    {
        assert_eq!(effect.event, EventKind::Shot, "{}", effect.name);
        assert!(
            effect.held_exit <= client::effects::catalog::MAX_SUSTAIN_EXIT,
            "{}",
            effect.name
        );
        assert!(
            effect.layers.iter().any(|layer| layer.held
                && layer.priority == client::effects::catalog::schema::Priority::Critical),
            "{} holds a Critical layer",
            effect.name
        );
    }
}

/// Plan WP11: a `Shot` effect binds its line and flight; a card may ride the line and be drawn
/// out along it, a strip may ride it with a trail, a layer may last a bound span; and each of
/// these is refused where it cannot apply.
#[test]
fn shot_layers_ride_their_line_and_last_their_flight() {
    let (dir, manifest) = fixture();
    let shot = |layer: &str| format!(r#""bolt": (class: Minor, event: Shot, layers: [ {layer} ])"#);
    let body = r#"(block: Sprite(texture: "soft_disc", ramp: "white", ride: true, stretch: 2.0),
        size: (scale: 0.1), duration: 0.6, lasts: FlightTime, priority: Critical)"#;
    let trail = r#"(block: Ribbon(texture: "solid", ramp: "white", trail: Some(0.5), shape: Radial),
        size: (bind: LineLength, scale: 0.01), duration: 0.6, lasts: FlightTime)"#;
    let catalog = load(
        dir.path(),
        &vocabulary(&manifest, "", &shot(&format!("{body}, {trail}")), ""),
    )
    .expect("a valid shot resolves");
    let effect = catalog
        .effect(catalog.effect_id("bolt").expect("defined"))
        .expect("resolves");
    assert_eq!(effect.event, EventKind::Shot);
    match &effect.layers[0].block {
        Block::Sprite(look) => assert!(look.ride && (look.stretch - 2.0).abs() < 1e-6),
        other => panic!("expected a sprite, got {other:?}"),
    }
    match &effect.layers[1].block {
        Block::Ribbon(look) => assert_eq!(look.trail, Some(0.5)),
        other => panic!("expected a ribbon, got {other:?}"),
    }
    assert_eq!(effect.layers[0].lasts, Some(Binding::FlightTime));
    let flight = client::effects::binding::BoundValues::default().with(Binding::FlightTime, 1.4);
    assert!(
        (effect.layers[0].span(&flight) - 1.4).abs() < 1e-6,
        "lasts the flight"
    );
    assert!(
        (effect.layers[0].span(&client::effects::binding::BoundValues::default()) - 0.6).abs()
            < 1e-6,
        "an instant shot keeps the duration"
    );
    assert!(
        (client::effects::runtime::one_shot_lifetime(effect, &flight) - 1.4).abs() < 1e-6,
        "the effect lives as long as it flies"
    );

    // Refusals: each names its field.
    let refused = [
        (
            r#"(block: Sprite(texture: "soft_disc", ramp: "white", orient: Ground, stretch: 2.0),
                size: (scale: 0.1), duration: 0.5)"#,
            "stretch",
        ),
        (
            r#"(block: Sprite(texture: "soft_disc", ramp: "white", stretch: 0.5),
                size: (scale: 0.1), duration: 0.5)"#,
            "stretch",
        ),
        (
            r#"(block: Ribbon(texture: "solid", ramp: "white", trail: Some(0.5), travel: 9.0),
                size: (scale: 0.1), duration: 0.5)"#,
            "trail",
        ),
        (
            r#"(block: Ribbon(texture: "solid", ramp: "white", trail: Some(0.0)),
                size: (scale: 0.1), duration: 0.5)"#,
            "trail",
        ),
        (
            r#"(block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 0.1),
                duration: 0.5, lasts: LineLength)"#,
            "lasts",
        ),
    ];
    for (layer, field) in refused {
        let error =
            load(dir.path(), &vocabulary(&manifest, "", &shot(layer), "")).expect_err(field);
        assert!(
            matches!(&error, EffectCatalogError::InvalidValue { field: named, .. } if *named == field),
            "{field}: {error}"
        );
    }
    // A splash has neither a flight to last nor a line to ride.
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(
                r#"(block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 0.1),
                    duration: 0.5, lasts: FlightTime)"#,
            ),
            "",
        ),
    )
    .expect_err("a splash has no flight");
    assert!(
        matches!(
            &error,
            EffectCatalogError::UnboundBinding {
                binding: Binding::FlightTime,
                event: EventKind::Splash,
                ..
            }
        ),
        "{error}"
    );
    let error = load(
        dir.path(),
        &vocabulary(
            &manifest,
            "",
            &splash_effect(
                r#"(block: Sprite(texture: "soft_disc", ramp: "white", ride: true),
                    size: (scale: 0.1), duration: 0.5)"#,
            ),
            "",
        ),
    )
    .expect_err("a splash has no line to ride");
    assert!(
        matches!(
            &error,
            EffectCatalogError::UnboundBinding {
                binding: Binding::LineLength,
                ..
            }
        ),
        "{error}"
    );
}

/// Plan WP11 (F1): a `Missile` effect holds its look and lays an `Area` impact; only a missile
/// lays an impact, and only an `Area` one.
#[test]
fn a_missile_holds_its_look_and_lays_an_area_impact() {
    let (dir, manifest) = fixture();
    let missile = |impact: &str, landed_kind: &str| {
        format!(
            r#""spore": (class: Minor, event: Missile, impact: Some("{impact}"),
                emit: Some((effect: "mote", every: 0.05)), layers: [
                (block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 0.2),
                 duration: 0.1, held: true, exit: 0.05, priority: Critical) ]),
            "mote": (class: Ambient, event: Missile, layers: [
                (block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 0.05),
                 duration: 0.3) ]),
            "landed": (class: Major, event: {landed_kind}, layers: [
                (block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 1.0),
                 duration: 0.5) ])"#
        )
    };
    let catalog = load(
        dir.path(),
        &vocabulary(&manifest, "", &missile("landed", "Area"), ""),
    )
    .expect("a valid missile resolves");
    let spore = catalog
        .effect(catalog.effect_id("spore").expect("defined"))
        .expect("resolves");
    assert_eq!(spore.impact, catalog.effect_id("landed"));
    assert!(spore.layers[0].held);
    let error = load(
        dir.path(),
        &vocabulary(&manifest, "", &missile("landed", "Point"), ""),
    )
    .expect_err("a missile lays an area");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue {
                field: "impact",
                ..
            }
        ),
        "{error}"
    );
    let not_a_missile = r#""boom": (class: Major, event: Splash, impact: Some("boom"), layers: [
        (block: Sprite(texture: "soft_disc", ramp: "white"), size: (scale: 1.0), duration: 0.5) ])"#;
    let error = load(dir.path(), &vocabulary(&manifest, "", not_a_missile, ""))
        .expect_err("only a missile lays an impact");
    assert!(
        matches!(
            &error,
            EffectCatalogError::InvalidValue {
                field: "impact",
                ..
            }
        ),
        "{error}"
    );
}
