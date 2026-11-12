//! Mode media (plan `docs/plans/2026-11-09-effect-visualization.md` WP13): the `modes` section
//! of `media_style.ron`, the prompts and pictures of mode clips, their ledger events and the mode
//! atlas bake.
//!
//! The tests discover every mode the shipped style names from the data; none names a unit or a
//! mode, so they keep covering the shipped modes when the data changes.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod common;

use asset_baker::ledger::{
    read_ledger_numbered, sha256_file, ApiMode, LedgerEvent, LedgerHistory, LedgerRecord,
};
use asset_baker::mode_media::{
    build_mode_prompt, catalog_mode, check_mode_clip, plan_mode_pictures, required_mode_slices,
    ModeAtlasId,
};
use asset_baker::prompts::{check_prompt, MediaStyle, MEDIA_STYLE_FILE};
use asset_baker::provenance::{bake_mode_with_provenance, veo_origin_of_mode_clip, SliceClip};
use asset_baker::veo_client::VeoParameters;
use asset_baker::{BakeFraming, BakerError, FlipbookAtlasMeta, FrameSpec, Morph, SliceKey};
use common::cli::{baker_with_assets, shipped_assets, shipped_catalog, text, write_picture};
use sim_types::AnimState;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn shipped_style() -> MediaStyle {
    MediaStyle::load_for_catalog(&shipped_assets(), &shipped_catalog())
        .expect("the shipped style loads and passes its checks")
}

/// Every mode the shipped style names, with its required slices.
fn shipped_modes() -> Vec<(String, String, Vec<SliceKey>)> {
    let catalog = shipped_catalog();
    let style = shipped_style();
    let style_path = shipped_assets().join(MEDIA_STYLE_FILE);
    let modes: Vec<_> = style
        .modes
        .modes()
        .map(|(id, _)| {
            let archetype = catalog.get_by_slug(id.slug).expect("an exact catalog slug");
            let (_, mode) =
                catalog_mode(archetype, id.stance, &style_path).expect("a catalog mode");
            (
                id.slug.to_string(),
                id.stance.to_string(),
                required_mode_slices(archetype, mode),
            )
        })
        .collect();
    assert!(!modes.is_empty(), "the shipped style names mode media");
    modes
}

/// A copy of the shipped catalog and media data with `media_style.ron` edited.
fn bundle_with_style(edit: impl Fn(String) -> String) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for sub in ["catalog", "media"] {
        std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        for entry in std::fs::read_dir(shipped_assets().join(sub)).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.path().join(sub).join(entry.file_name())).unwrap();
        }
    }
    let style = dir.path().join(MEDIA_STYLE_FILE);
    std::fs::write(&style, edit(std::fs::read_to_string(&style).unwrap())).unwrap();
    dir
}

/// The error of checking an edited style against the shipped catalog.
fn style_error(edit: impl Fn(String) -> String) -> String {
    let bundle = bundle_with_style(edit);
    match MediaStyle::load_for_catalog(bundle.path(), &shipped_catalog()) {
        Ok(_) => panic!("the edited style was accepted"),
        Err(error) => error.to_string(),
    }
}

/// The first shipped mode entry's opening line, `"<mode>": (`, and its slug.
fn first_mode_entry() -> (String, String) {
    let (slug, stance, _) = shipped_modes().remove(0);
    (slug, format!("\"{stance}\": ("))
}

#[test]
fn shipped_modes_need_their_idle_and_attack_and_both_morphs() {
    for (slug, stance, required) in shipped_modes() {
        assert!(
            required.contains(&SliceKey::Action(AnimState::Idle)),
            "{slug}-{stance}"
        );
        assert!(
            required.contains(&SliceKey::Morph(Morph::Enter))
                && required.contains(&SliceKey::Morph(Morph::Leave)),
            "{slug}-{stance}"
        );
        let catalog = shipped_catalog();
        let archetype = catalog.get_by_slug(&slug).unwrap();
        let mode = archetype
            .modes
            .iter()
            .find(|mode| mode.mode_name == stance)
            .unwrap();
        assert_eq!(
            required.contains(&SliceKey::Action(AnimState::Walk)),
            !mode.is_immobile,
            "{slug}-{stance}: Walk exactly when the mode can move"
        );
    }
}

#[test]
fn every_shipped_mode_prompt_passes_the_prompt_rules() {
    let catalog = shipped_catalog();
    let style = shipped_style();
    let style_path = shipped_assets().join(MEDIA_STYLE_FILE);
    for (slug, stance, required) in shipped_modes() {
        let id = ModeAtlasId {
            slug: &slug,
            stance: &stance,
        };
        for slot in required {
            let prompt = build_mode_prompt(&catalog, &style, id, slot, &style_path)
                .expect("every required slice has a text");
            assert!(prompt.authored, "{} {slot}", id.atlas_name());
            assert!(
                prompt.text.ends_with(&style.style),
                "{} {slot}: the style sentence ends the prompt",
                id.atlas_name()
            );
            assert_eq!(
                check_prompt(&style, &prompt),
                Vec::new(),
                "{} {slot}",
                id.atlas_name()
            );
        }
    }
}

#[test]
fn a_mode_entry_for_an_unknown_unit_or_mode_is_refused() {
    let (slug, entry) = first_mode_entry();
    // The slug also keys other sections' overrides; rename only its `modes` entry.
    let error = style_error(|style| {
        let at = style
            .find("    modes: {")
            .expect("the shipped style has a modes section");
        let (before, after) = style.split_at(at);
        format!(
            "{before}{}",
            after.replacen(&format!("\"{slug}\": {{"), "\"no_such_unit\": {", 1)
        )
    });
    assert!(error.contains("modes: 'no_such_unit'"), "{error}");
    let error = style_error(|style| style.replacen(&entry, "\"no_such_mode\": (", 1));
    assert!(error.contains("no_such_mode"), "{error}");
}

#[test]
fn a_mode_without_a_text_for_a_state_it_shows_is_refused() {
    let style = std::fs::read_to_string(shipped_assets().join(MEDIA_STYLE_FILE)).unwrap();
    let (_, entry) = first_mode_entry();
    let at = style.find(&entry).unwrap();
    let idle = at + style[at..].find("Idle: \"").unwrap();
    let error = style_error(|text| {
        let mut text = text;
        text.replace_range(idle..idle + "Idle".len(), "Cast");
        text
    });
    assert!(error.contains("idle") || error.contains("Cast"), "{error}");
}

#[test]
fn a_seam_on_a_state_that_does_not_loop_is_refused() {
    let style = std::fs::read_to_string(shipped_assets().join(MEDIA_STYLE_FILE)).unwrap();
    let (_, entry) = first_mode_entry();
    let at = style.find(&entry).unwrap();
    let attack = at
        + style[at..]
            .find("Attack: (pin_end: true, seam: None)")
            .unwrap();
    let error = style_error(|text| {
        let mut text = text;
        let old = "Attack: (pin_end: true, seam: None)";
        text.replace_range(
            attack..attack + old.len(),
            "Attack: (pin_end: true, seam: Some((scale_lock: true, crossfade_frames: 0)))",
        );
        text
    });
    assert!(error.contains("does not loop"), "{error}");
}

#[test]
fn a_cross_fade_beyond_a_quarter_of_the_mode_budget_is_refused() {
    let style = std::fs::read_to_string(shipped_assets().join(MEDIA_STYLE_FILE)).unwrap();
    let (_, entry) = first_mode_entry();
    let at = style.find(&entry).unwrap();
    let idle = at
        + style[at..]
            .find("Idle: (pin_end: true, seam: Some((")
            .unwrap();
    let end = idle + style[idle..].find("))),").unwrap() + "))),".len();
    let error = style_error(|text| {
        let mut text = text;
        text.replace_range(
            idle..end,
            "Idle: (pin_end: true, seam: Some((scale_lock: true, crossfade_frames: 4))),",
        );
        text
    });
    assert!(error.contains("cross-fades 4 frames"), "{error}");
}

#[test]
fn pictures_chain_the_morphs_through_the_mode_picture() {
    let dir = tempfile::tempdir().unwrap();
    let style = shipped_style();
    let (slug, stance, _) = shipped_modes().remove(0);
    let id = ModeAtlasId {
        slug: &slug,
        stance: &stance,
    };
    let spec = style.modes.spec(id).unwrap();
    let idle = dir.path().join("idle_picture.png");
    write_picture(&idle, 1280, 720);
    let enter = SliceKey::Morph(Morph::Enter);
    let leave = SliceKey::Morph(Morph::Leave);
    let state = SliceKey::Action(AnimState::Idle);

    // Before the enter morph exists, it seeds the mode's picture and nothing else can start.
    let first = plan_mode_pictures(spec, id, enter, &idle, dir.path()).unwrap();
    assert_eq!((first.start.as_path(), first.end), (idle.as_path(), None));
    for slot in [state, leave] {
        assert!(matches!(
            plan_mode_pictures(spec, id, slot, &idle, dir.path()),
            Err(BakerError::DataFile { .. })
        ));
    }

    let picture = id.picture(dir.path());
    write_picture(&picture, 1280, 720);
    let pinned = plan_mode_pictures(spec, id, state, &idle, dir.path()).unwrap();
    assert_eq!(pinned.start, picture);
    assert_eq!(
        pinned.end.as_deref(),
        Some(picture.as_path()),
        "a pinned loop"
    );
    let back = plan_mode_pictures(spec, id, leave, &idle, dir.path()).unwrap();
    assert_eq!(
        (back.start, back.end),
        (picture.clone(), Some(idle.clone()))
    );
    let again = plan_mode_pictures(spec, id, enter, &idle, dir.path()).unwrap();
    assert_eq!(
        again.end,
        Some(picture.clone()),
        "a regeneration ends on the picture"
    );

    let prompt = build_mode_prompt(
        &shipped_catalog(),
        &style,
        id,
        state,
        &shipped_assets().join(MEDIA_STYLE_FILE),
    )
    .unwrap();
    assert_eq!(check_mode_clip(&style, &prompt, &pinned), Vec::new());
    write_picture(&picture, 512, 512);
    assert!(
        !check_mode_clip(&style, &prompt, &pinned).is_empty(),
        "a square picture cannot pin a clip's last frame"
    );
}

fn record(event: LedgerEvent) -> LedgerRecord {
    LedgerRecord {
        time_utc: "2026-11-12T00:00:00Z".into(),
        unix_seconds: 0,
        run_id: "r".into(),
        event,
    }
}

fn parameters() -> VeoParameters {
    VeoParameters {
        aspect_ratio: Some("16:9".into()),
        duration_seconds: Some(4),
        sample_count: Some(1),
        generate_audio: None,
        negative_prompt: None,
        seed: None,
        resolution: None,
    }
}

fn mode_started(request_id: &str, slot: SliceKey) -> LedgerEvent {
    LedgerEvent::ModeClipRequestStarted {
        request_id: request_id.into(),
        slug: "fixture_unit".into(),
        stance: "fixture_mode".into(),
        slot,
        model: "fixture-model".into(),
        mode: ApiMode::VertexExpress,
        prompt_sha256: "p".into(),
        prompt: "prompt".into(),
        keyframe_sha256: Some("k".into()),
        last_frame_sha256: Some("e".into()),
        parameters: parameters(),
    }
}

fn mode_downloaded(operation: &str, slot: SliceKey, sha256: &str) -> LedgerEvent {
    LedgerEvent::ModeClipDownloaded {
        slug: "fixture_unit".into(),
        stance: "fixture_mode".into(),
        slot,
        operation: operation.into(),
        bytes: 1,
        sha256: sha256.into(),
        duration_seconds: 4.0,
        frames: 96,
        width: 1280,
        height: 720,
        path: "clips/fixture.mp4".into(),
    }
}

#[test]
fn mode_clip_events_round_trip_and_fold_into_their_own_history() {
    let enter = SliceKey::Morph(Morph::Enter);
    let idle = SliceKey::Action(AnimState::Idle);
    let events = vec![
        mode_started("r/1", enter),
        LedgerEvent::ModeClipOperationNamed {
            request_id: "r/1".into(),
            slug: "fixture_unit".into(),
            stance: "fixture_mode".into(),
            slot: enter,
            model: "fixture-model".into(),
            operation: "op-1".into(),
        },
        mode_downloaded("op-1", enter, "aaa"),
        LedgerEvent::ModeClipRejected {
            slug: "fixture_unit".into(),
            stance: "fixture_mode".into(),
            slot: enter,
            sha256: "aaa".into(),
            reason: "claws detach".into(),
        },
        mode_started("r/2", idle),
        LedgerEvent::ModeClipQuotaRefused {
            request_id: "r/2".into(),
            slug: "fixture_unit".into(),
            stance: "fixture_mode".into(),
            slot: idle,
            http_status: 429,
            status: None,
            reason: None,
        },
    ];
    let records: Vec<LedgerRecord> = events.into_iter().map(record).collect();
    for line in &records {
        let json = serde_json::to_string(line).unwrap();
        assert_eq!(&serde_json::from_str::<LedgerRecord>(&json).unwrap(), line);
    }
    let json = serde_json::to_string(&records[0]).unwrap();
    assert!(
        json.contains("\"event\":\"mode_clip_request_started\""),
        "{json}"
    );
    assert!(json.contains("\"slot\":\"enter\""), "{json}");

    let history = LedgerHistory::from_records(&records);
    let entered = history
        .mode_clip("fixture_unit", "fixture_mode", enter)
        .unwrap();
    assert_eq!(entered.paid_attempts, 1);
    assert!(
        entered.downloaded.is_none(),
        "the rejected download is gone"
    );
    assert!(entered.rejected_sha256.contains("aaa"));
    assert_eq!(
        history
            .mode_clip("fixture_unit", "fixture_mode", idle)
            .unwrap()
            .paid_attempts,
        0,
        "a quota refusal costs nothing"
    );
    assert!(history.clip("fixture_unit", AnimState::Idle).is_none());
    assert_eq!(
        history.paid_start_models().collect::<Vec<_>>(),
        ["fixture-model"],
        "the budget counts the paid mode start once"
    );
}

#[test]
fn the_shipped_ledger_prices_every_mode_start() {
    let ledger = shipped_assets().join("../docs/evidence/veo/ledger.jsonl");
    let history = LedgerHistory::load(&ledger).expect("the committed ledger reads");
    let budget = asset_baker::budget::BudgetConfig::load(
        &shipped_assets().join(asset_baker::budget::BUDGET_FILE),
    )
    .unwrap();
    let spent = budget.spent_cents(&history).expect("every model is priced");
    assert!(spent <= budget.total_budget_usd_cents, "{spent} cents");
}

#[test]
fn a_mode_dry_run_prints_the_request_and_refuses_a_slice_the_mode_never_shows() {
    let (slug, stance, required) = shipped_modes().remove(0);
    let dir = tempfile::tempdir().unwrap();
    let ledger = dir.path().join("ledger.jsonl");
    let run = |slice: &str| {
        baker_with_assets(&shipped_assets())
            .current_dir(dir.path())
            .args([
                "mode-veo", "--unit", &slug, "--mode", &stance, "--slice", slice,
            ])
            .arg("--keyframes-dir")
            .arg(shipped_assets().join("ai_keyframes"))
            .arg("--ledger")
            .arg(&ledger)
            .arg("--out-dir")
            .arg(dir.path().join("clips"))
            .args(["--max-generations", "1", "--dry-run"])
            .output()
            .unwrap()
    };
    let output = run("enter");
    assert!(output.status.success(), "{}", text(&output));
    assert!(text(&output).contains("Dry run"), "{}", text(&output));
    assert!(!ledger.exists(), "a dry run writes no ledger line");

    let never = ["walk", "cast", "constructing"]
        .into_iter()
        .find(|slice| !required.contains(&SliceKey::parse(slice).unwrap()))
        .expect("an immobile mode never shows a walk");
    let output = run(never);
    assert!(!output.status.success());
    assert!(text(&output).contains("never shows"), "{}", text(&output));
}

/// The fixture clip, copied into `videos` once per slice under its installed name.
fn install_fixture_clips(videos: &Path, id: ModeAtlasId<'_>, slots: &[SliceKey]) -> Vec<PathBuf> {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/luma_ramp_4s.mp4");
    std::fs::create_dir_all(videos).unwrap();
    slots
        .iter()
        .map(|slot| {
            let path = videos.join(id.clip_file(*slot));
            std::fs::copy(&fixture, &path).unwrap();
            path
        })
        .collect()
}

#[test]
fn a_mode_atlas_bakes_its_states_and_morphs_with_their_origins() {
    let dir = tempfile::tempdir().unwrap();
    let id = ModeAtlasId {
        slug: "fixture_unit",
        stance: "fixture_mode",
    };
    let slots = [
        SliceKey::Action(AnimState::Idle),
        SliceKey::Action(AnimState::Attack),
        SliceKey::Morph(Morph::Enter),
        SliceKey::Morph(Morph::Leave),
    ];
    let videos = dir.path().join("videos");
    let paths = install_fixture_clips(&videos, id, &slots);
    let sha256 = sha256_file(&paths[0]).unwrap();
    let ledger_path = dir.path().join("ledger.jsonl");
    let lines: Vec<String> = slots
        .iter()
        .enumerate()
        .flat_map(|(index, slot)| {
            let operation = format!("op-{index}");
            let request = format!("r/{index}");
            [
                mode_started(&request, *slot),
                LedgerEvent::ModeClipOperationNamed {
                    request_id: request.clone(),
                    slug: "fixture_unit".into(),
                    stance: "fixture_mode".into(),
                    slot: *slot,
                    model: "fixture-model".into(),
                    operation: operation.clone(),
                },
                mode_downloaded(&operation, *slot, &sha256),
            ]
        })
        .map(|event| serde_json::to_string(&record(event)).unwrap())
        .collect();
    std::fs::write(&ledger_path, lines.join("\n") + "\n").unwrap();
    let ledger = read_ledger_numbered(&ledger_path).unwrap();
    let clips: Vec<SliceClip> = slots
        .iter()
        .zip(&paths)
        .map(|(slot, path)| SliceClip {
            slot: *slot,
            path: path.clone(),
            origin: veo_origin_of_mode_clip(&ledger, id.slug, id.stance, *slot, &sha256)
                .expect("the ledger downloaded every clip"),
        })
        .collect();
    let textures = dir.path().join("textures");
    let seams = BTreeMap::new();
    let spec = FrameSpec {
        size: 32,
        fps: 5,
        framing: BakeFraming::CenterCrop,
    };
    let record =
        bake_mode_with_provenance(&id.atlas_name(), &clips, &videos, &textures, spec, &seams)
            .expect("the mode atlas bakes");

    let meta = FlipbookAtlasMeta::load_from_ron(
        &textures.join(format!("{}.flipbook.ron", id.atlas_name())),
    )
    .unwrap();
    assert_eq!(meta.name, id.atlas_name());
    assert_eq!(
        meta.actions.keys().copied().collect::<Vec<_>>(),
        [AnimState::Idle, AnimState::Attack]
    );
    assert_eq!(
        meta.morphs.keys().copied().collect::<Vec<_>>(),
        [Morph::Enter, Morph::Leave]
    );
    for morph in meta.morphs.values() {
        assert!(
            !morph.looping && morph.hold_last_frame,
            "a morph plays once and holds"
        );
    }
    assert!(meta.actions[&AnimState::Idle].looping);
    assert!(meta.morphs[&Morph::Enter].start_frame >= meta.actions[&AnimState::Attack].start_frame);
    assert_eq!(record.states.len(), 2);
    assert_eq!(record.morphs.len(), 2);
    assert_eq!(
        record.morphs[&Morph::Leave].clip,
        id.clip_file(SliceKey::Morph(Morph::Leave))
    );
    assert!(matches!(
        record.morphs[&Morph::Enter].origin,
        asset_baker::provenance::ClipOrigin::Veo { ledger_line: 9, .. }
    ));
    let text =
        std::fs::read_to_string(textures.join(format!("{}.provenance.ron", id.atlas_name())))
            .unwrap();
    assert!(text.contains("morphs:"), "{text}");
}

#[test]
fn a_mode_clip_outside_the_videos_directory_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let id = ModeAtlasId {
        slug: "fixture_unit",
        stance: "fixture_mode",
    };
    let elsewhere = install_fixture_clips(
        &dir.path().join("elsewhere"),
        id,
        &[SliceKey::Morph(Morph::Enter)],
    );
    std::fs::create_dir_all(dir.path().join("videos")).unwrap();
    let clips = [SliceClip {
        slot: SliceKey::Morph(Morph::Enter),
        path: elsewhere[0].clone(),
        origin: asset_baker::provenance::ClipOrigin::LegacyVeo {
            commit: "0".repeat(40),
        },
    }];
    let result = bake_mode_with_provenance(
        &id.atlas_name(),
        &clips,
        &dir.path().join("videos"),
        &dir.path().join("textures"),
        FrameSpec {
            size: 32,
            fps: 5,
            framing: BakeFraming::CenterCrop,
        },
        &BTreeMap::new(),
    );
    assert!(matches!(
        result,
        Err(asset_baker::provenance::ProvenanceError::SliceOutsideVideos { .. })
    ));
}
