//! The effect vocabulary loader through its public API (plan
//! `2026-11-09-effect-visualization` WP4, §5.2, §5.3, D-12).
//!
//! # Responsibility
//! One broken fixture per [`EffectCatalogError`] variant, each checking the variant and that
//! it names the file, effect and layer; the shipped vocabulary and every shipped unit's
//! presentation block resolve from the runtime path; and a copy of `effects.ron` edited in a
//! temporary directory loads a different effect with no rebuild (charter C6).
//!
//! # Invariants verified
//! - Every failure is typed and names where it is.
//! - Names resolve to dense ids in file order; effect lifetimes cover every layer.

#![allow(clippy::expect_used, clippy::panic)]

#[path = "support/effect_fixtures.rs"]
mod effect_fixtures;

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
    );
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

#[test]
fn the_shipped_vocabulary_and_presentation_blocks_resolve_from_the_runtime_path() {
    let root = ResolvedAssetsRoot::resolve().expect("the assets root resolves");
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .expect("the shipped vocabulary loads");
    let units = sim_fixtures::shipped_catalogs().units;
    UnitEffectTables::build(&units, &catalog).expect("every shipped presentation block resolves");
    assert!(catalog.budget.effects > 0 && catalog.budget.sprites > 0);
}

/// The shipped unit catalog with the first archetype's presentation block replaced.
fn units_with_block(
    block: ArchetypeEffects,
) -> (sim_core::catalog::UnitCatalog, sim_types::UnitTypeId) {
    let mut units = sim_fixtures::shipped_catalogs().units;
    let id = units
        .archetypes()
        .next()
        .expect("the shipped catalog has an archetype")
        .unit_type_id;
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
    let (dir, manifest) = fixture();
    let copy = dir.path().join("effects.ron");
    std::fs::write(&copy, &shipped).expect("the copy is written");
    let before = EffectCatalog::load(&copy, root.as_path()).expect("the copy loads");
    assert!(before.effect_id("boom").is_none());

    let edited = shipped
        .replace("textures: [],", &format!("textures: [{manifest:?}],"))
        .replace(
            "ramps: {},",
            r#"ramps: { "white": [(0.0, (1.0, 1.0, 1.0, 1.0)), (1.0, (1.0, 1.0, 1.0, 1.0))] },"#,
        )
        .replace(
            "effects: {},",
            r#"effects: { "boom": (class: Major, event: Splash, layers: [
                (block: Sprite(texture: "soft_disc", ramp: "white"),
                 size: (bind: SplashReach, scale: 2.5), duration: 0.75, priority: Critical) ]) },"#,
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
