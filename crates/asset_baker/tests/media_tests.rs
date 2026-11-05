//! Exact-media admission and truthful read-only audit through public APIs.
//!
//! Real checked-in files establish availability; tiny test PNGs exercise only
//! malformed-input algorithms and never serve as game or visual acceptance art.

use asset_baker::{
    media::{load_unit_media, media_paths, require_action, MediaLoadError, TextureLimits},
    media_audit::{audit_media, AuditInputs, MediaAuditStatus},
    provenance::{RequiredSet, REQUIRED_SET_FILE},
    ActionSlice, FlipbookAtlasMeta, FlipbookLayout,
};
use sim_core::catalog::RuntimeCatalogs;
use sim_types::AnimState;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn fixture() -> FlipbookAtlasMeta {
    FlipbookAtlasMeta {
        name: "fixture_identity".into(),
        frame_width: 2,
        frame_height: 2,
        fps: 12,
        total_frames: 2,
        layout: FlipbookLayout::VerticalStrip,
        actions: BTreeMap::from([(
            AnimState::Idle,
            ActionSlice {
                start_frame: 0,
                frame_count: 2,
                looping: true,
                hold_last_frame: false,
                duration_ms: None,
            },
        )]),
    }
}

fn write_meta(directory: &Path, meta: &FlipbookAtlasMeta) -> TestResult {
    std::fs::write(
        directory.join("fixture_identity.flipbook.ron"),
        ron::to_string(meta)?,
    )?;
    Ok(())
}

/// The committed ledger of every Veo request (`docs/evidence/veo/ledger.jsonl`).
fn shipped_ledger() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/evidence/veo/ledger.jsonl")
}

#[test]
fn real_catalog_media_audit_is_ordered_and_strict_passes_only_with_the_ledger() -> TestResult {
    let catalogs = RuntimeCatalogs::load_from_directory(root().join("catalog"))?;
    let required = RequiredSet::load(&root().join(REQUIRED_SET_FILE), &catalogs.units)?;
    // Batches 1 and 2 (Veo plan WP4, WP5) gave every required identity verified media, but a
    // `Veo` origin is verified only against its `downloaded` line in the ledger.
    let unverified = audit_media(
        &root(),
        &catalogs.units,
        &AuditInputs {
            required: &required,
            ledger: None,
        },
    );
    assert!(!unverified.strict_accepted);
    assert!(matches!(
        unverified.require_verified(),
        Err(asset_baker::media_audit::MediaAuditError::Unverified { identities }) if !identities.is_empty()
    ));
    let ledger = asset_baker::ledger::read_ledger_numbered(&shipped_ledger())?;
    let report = audit_media(
        &root(),
        &catalogs.units,
        &AuditInputs {
            required: &required,
            ledger: Some(&ledger),
        },
    );
    assert_eq!(report.entries.len(), catalogs.units.archetypes().count());
    assert!(report
        .entries
        .windows(2)
        .all(|pair| pair[0].slug < pair[1].slug));
    assert!(report.strict_accepted);
    assert!(report.require_verified().is_ok());
    let output = tempfile::tempdir()?;
    let report_path = output.path().join("audit.json");
    report.write_json(&report_path)?;
    let original = std::fs::read(&report_path)?;
    assert!(
        matches!(report.write_json(&report_path), Err(asset_baker::media_audit::MediaAuditError::Io { path, .. }) if path == report_path)
    );
    assert_eq!(std::fs::read(&report_path)?, original);
    let invalid_path = output.path().join("missing/audit.json");
    assert!(
        matches!(report.write_json(&invalid_path), Err(asset_baker::media_audit::MediaAuditError::Io { path, .. }) if path == invalid_path)
    );

    assert!(report.concept_failure.is_none());
    assert!(report
        .entries
        .iter()
        .any(|entry| entry.status == MediaAuditStatus::MissingAsset));
    // Batch 2 replaced the last copies of the concept atlas (B-1).
    assert!(report
        .entries
        .iter()
        .all(|entry| entry.status != MediaAuditStatus::KnownConceptDuplicate));
    assert_eq!(report.duplicate_groups.len(), 0);
    let real = report
        .entries
        .iter()
        .find(|entry| entry.status == MediaAuditStatus::LegacyUnverified)
        .ok_or("no own legacy media")?;
    let loaded = load_unit_media(&root().join("textures"), &real.slug, None)?;
    assert_eq!(real.png_digest.as_ref(), Some(&loaded.png_digest));
    assert_eq!(
        loaded.png_digest,
        blake3::hash(&std::fs::read(
            root().join("textures").join(format!("{}.png", real.slug))
        )?)
        .to_hex()
        .as_str()
    );
    assert_eq!(real.alpha_range, Some(loaded.alpha_range));
    Ok(())
}

#[test]
fn no_shipped_texture_is_a_copy_of_the_concept_atlas() -> TestResult {
    // B-1 (Veo plan WP5 step 2): identities once shipped byte-identical copies of the concept
    // atlas. Only the concept atlas itself (the Visual Lab's demo) may hold those bytes.
    let textures = root().join("textures");
    let concept = std::fs::read(textures.join("concept_video.png"))?;
    let mut copies = Vec::new();
    for entry in std::fs::read_dir(&textures)? {
        let path = entry?.path();
        let is_png = path.extension().is_some_and(|extension| extension == "png");
        let is_concept = path
            .file_name()
            .is_some_and(|name| name == "concept_video.png");
        if path.is_file() && is_png && !is_concept && std::fs::read(&path)? == concept {
            copies.push(path);
        }
    }
    assert!(copies.is_empty(), "copies of the concept atlas: {copies:?}");
    Ok(())
}

#[test]
fn representation_checks_dimensions_actions_identity_and_device_limits() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut meta = fixture();
    write_meta(dir.path(), &meta)?;
    image::RgbaImage::new(2, 4).save(dir.path().join("fixture_identity.png"))?;
    let loaded = load_unit_media(dir.path(), &meta.name, None)?;
    assert!(matches!(
        require_action(
            &loaded.metadata,
            &dir.path().join("fixture_identity.flipbook.ron"),
            AnimState::Attack
        ),
        Err(MediaLoadError::MissingAction {
            action: AnimState::Attack,
            ..
        })
    ));
    assert!(matches!(
        load_unit_media(
            dir.path(),
            &meta.name,
            Some(TextureLimits {
                max_dimension_2d: 3,
                max_array_layers: 2
            })
        ),
        Err(MediaLoadError::Representation { .. })
    ));
    assert!(matches!(
        load_unit_media(
            dir.path(),
            &meta.name,
            Some(TextureLimits {
                max_dimension_2d: 4,
                max_array_layers: 1
            })
        ),
        Err(MediaLoadError::Representation { .. })
    ));
    // Same pixel count but wrong actual dimensions used to be admitted by the client.
    image::RgbaImage::new(4, 2).save(dir.path().join("fixture_identity.png"))?;
    assert!(matches!(
        load_unit_media(dir.path(), &meta.name, None),
        Err(MediaLoadError::Representation { .. })
    ));
    meta.name = "different_identity".into();
    write_meta(dir.path(), &meta)?;
    assert!(matches!(
        load_unit_media(dir.path(), "fixture_identity", None),
        Err(MediaLoadError::Representation { .. })
    ));
    meta = fixture();
    meta.frame_height = u32::MAX;
    write_meta(dir.path(), &meta)?;
    assert!(matches!(
        load_unit_media(dir.path(), &meta.name, None),
        Err(MediaLoadError::Representation { .. })
    ));
    meta = fixture();
    meta.total_frames = u32::MAX;
    meta.actions
        .get_mut(&AnimState::Idle)
        .ok_or("fixture idle")?
        .start_frame = u32::MAX;
    write_meta(dir.path(), &meta)?;
    assert!(matches!(
        load_unit_media(dir.path(), &meta.name, None),
        Err(MediaLoadError::Representation { .. })
    ));
    Ok(())
}

#[test]
fn missing_corrupt_and_ambiguous_metadata_preserve_exact_paths() -> TestResult {
    let dir = tempfile::tempdir()?;
    let meta = fixture();
    assert!(
        matches!(load_unit_media(dir.path(), &meta.name, None), Err(MediaLoadError::Io { path, .. }) if path == dir.path().join("fixture_identity.flipbook.ron"))
    );
    write_meta(dir.path(), &meta)?;
    assert!(
        matches!(load_unit_media(dir.path(), &meta.name, None), Err(MediaLoadError::Io { path, .. }) if path == dir.path().join("fixture_identity.png"))
    );
    std::fs::write(dir.path().join("fixture_identity.png"), "not a PNG")?;
    assert!(matches!(
        load_unit_media(dir.path(), &meta.name, None),
        Err(MediaLoadError::Image { .. })
    ));
    let encoded = ron::to_string(&meta)?;
    let action = ron::to_string(meta.actions.get(&AnimState::Idle).ok_or("fixture idle")?)?;
    let duplicate = encoded.replace("actions:{", &format!("actions:{{Idle:{action},"));
    std::fs::write(dir.path().join("fixture_identity.flipbook.ron"), duplicate)?;
    assert!(matches!(
        load_unit_media(dir.path(), &meta.name, None),
        Err(MediaLoadError::Metadata { .. })
    ));
    let unknown = encoded.replacen('(', "(unsupported_control:1,", 1);
    std::fs::write(dir.path().join("fixture_identity.flipbook.ron"), unknown)?;
    assert!(matches!(
        load_unit_media(dir.path(), &meta.name, None),
        Err(MediaLoadError::Metadata { .. })
    ));
    Ok(())
}

#[test]
fn portable_paths_reject_reserved_and_traversing_identities() {
    for slug in [
        "",
        " ",
        ".",
        "..",
        "../escape",
        "a/b",
        "a\\b",
        "a:b",
        "a?b",
        "a*b",
        "a|b",
        "a<b",
        "a>b",
        "a\"b",
        "a\nb",
        "trailing.",
        "trailing ",
        "CON",
        "con.ext",
        "NUL",
        "AUX",
        "PRN",
        "COM1",
        "lpt9",
        "COM¹",
    ] {
        assert!(
            matches!(
                media_paths(Path::new("textures"), slug),
                Err(MediaLoadError::InvalidSlug { .. })
            ),
            "{slug:?}"
        );
    }
    assert!(media_paths(Path::new("textures"), "Fixture-Identity_42").is_ok());
}

#[test]
#[allow(deprecated)]
fn retired_batch_and_cli_do_not_write_or_fall_back() -> TestResult {
    let dir = tempfile::tempdir()?;
    let output = dir.path().join("uncreated");
    assert!(matches!(
        asset_baker::bake_starter_units_from_real_video(Path::new("missing.mp4"), &output, 512, 15),
        Err(asset_baker::BakerError::ExplicitSourcesRequired)
    ));
    assert!(!output.exists());
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_asset_baker"))
        .args([
            "generate-defaults",
            "--concept-mp4",
            "missing.mp4",
            "--output-dir",
        ])
        .arg(&output)
        .output()?;
    assert!(!result.status.success());
    assert!(!output.exists());
    Ok(())
}
