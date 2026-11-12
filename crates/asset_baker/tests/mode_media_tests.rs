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
    ModeAtlasId, Reframe,
};
use asset_baker::prompts::{check_prompt, MediaStyle, MEDIA_STYLE_FILE};
use asset_baker::provenance::{
    bake_mode_with_provenance, veo_origin_of_mode_clip, verify_mode_provenance, ModeVerifyContext,
    SliceClip, UnverifiedReason,
};
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

#[test]
fn a_reframed_mode_chains_its_morphs_through_the_reframed_idle_picture() {
    let dir = tempfile::tempdir().unwrap();
    let style = shipped_style();
    let (slug, stance, _) = shipped_modes().remove(0);
    let id = ModeAtlasId {
        slug: &slug,
        stance: &stance,
    };
    let mut spec = style.modes.spec(id).unwrap().clone();
    spec.reframe = Some(Reframe {
        scale_permille: 700,
        picture_feather_permille: 60,
    });
    let idle = dir.path().join("idle_picture.png");
    write_picture(&idle, 1280, 720);
    let enter = SliceKey::Morph(Morph::Enter);
    let leave = SliceKey::Morph(Morph::Leave);

    // Without its reframed Idle picture, a reframed mode's morphs cannot start or end.
    let missing = plan_mode_pictures(&spec, id, enter, &idle, dir.path());
    assert!(
        matches!(&missing, Err(BakerError::DataFile { reason, .. }) if reason.contains("reframe-mode-picture")),
        "{missing:?}"
    );

    let reframed = id.reframed_idle_picture(dir.path());
    write_picture(&reframed, 1280, 720);
    let first = plan_mode_pictures(&spec, id, enter, &idle, dir.path()).unwrap();
    assert_eq!((first.start, first.end), (reframed.clone(), None));
    write_picture(&id.picture(dir.path()), 1280, 720);
    let back = plan_mode_pictures(&spec, id, leave, &idle, dir.path()).unwrap();
    assert_eq!(
        (back.start, back.end),
        (id.picture(dir.path()), Some(reframed))
    );
    let state = plan_mode_pictures(
        &spec,
        id,
        SliceKey::Action(AnimState::Idle),
        &idle,
        dir.path(),
    )
    .unwrap();
    assert_eq!(
        state.start,
        id.picture(dir.path()),
        "states start on the mode's picture"
    );
}

#[test]
fn a_reframed_mode_bakes_with_the_reframed_letterbox_of_its_unit() {
    let style = shipped_style();
    let path = shipped_assets().join(MEDIA_STYLE_FILE);
    let (slug, stance, _) = shipped_modes().remove(0);
    let id = ModeAtlasId {
        slug: &slug,
        stance: &stance,
    };
    let mut spec = style.modes.spec(id).unwrap().clone();
    let letterbox = BakeFraming::Letterbox {
        feather_permille: 30,
    };
    spec.reframe = None;
    assert_eq!(spec.bake_framing(letterbox, &path).unwrap(), letterbox);
    spec.reframe = Some(Reframe {
        scale_permille: 650,
        picture_feather_permille: 0,
    });
    assert_eq!(
        spec.bake_framing(letterbox, &path).unwrap(),
        BakeFraming::ReframedLetterbox {
            feather_permille: 30,
            scale_permille: 650,
        }
    );
    assert!(matches!(
        spec.bake_framing(BakeFraming::CenterCrop, &path),
        Err(BakerError::DataFile { .. })
    ));
}

/// The shipped style with `reframe` set on the first mode entry.
fn with_first_mode_reframe(text: String, reframe: &str) -> String {
    let (_, entry) = first_mode_entry();
    let at = text.find(&entry).unwrap() + entry.len();
    let mut text = text;
    text.insert_str(at, &format!("\n                reframe: Some({reframe}),"));
    text
}

#[test]
fn a_reframe_out_of_range_is_refused() {
    for (reframe, wanted) in [
        (
            "(scale_permille: 400, picture_feather_permille: 60)",
            "scale_permille 400",
        ),
        (
            "(scale_permille: 1000, picture_feather_permille: 60)",
            "scale_permille 1000",
        ),
        (
            "(scale_permille: 700, picture_feather_permille: 501)",
            "picture_feather_permille 501",
        ),
    ] {
        let error = style_error(|text| with_first_mode_reframe(text, reframe));
        assert!(error.contains(wanted), "{error}");
    }
}

#[test]
fn a_reframe_of_a_centre_cropped_unit_is_refused_before_any_spend() {
    let (slug, stance, _) = shipped_modes().remove(0);
    let bundle = bundle_with_style(|text| {
        let text = text.replace(&format!("\"{slug}\": Letterbox(feather_permille: 30),"), "");
        with_first_mode_reframe(text, "(scale_permille: 700, picture_feather_permille: 60)")
    });
    let dir = tempfile::tempdir().unwrap();
    let output = baker_with_assets(bundle.path())
        .current_dir(dir.path())
        .args([
            "mode-veo", "--unit", &slug, "--mode", &stance, "--slice", "enter",
        ])
        .arg("--ledger")
        .arg(dir.path().join("ledger.jsonl"))
        .arg("--out-dir")
        .arg(dir.path().join("clips"))
        .args(["--max-generations", "1", "--dry-run"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        text(&output).contains("needs a letterboxed unit"),
        "{}",
        text(&output)
    );
}

#[test]
fn every_shipped_mode_atlas_records_its_reframe() {
    let style = shipped_style();
    let textures = shipped_assets().join("textures");
    let keyframes = shipped_assets().join("ai_keyframes");
    let mut checked = 0;
    for (id, spec) in style.modes.modes() {
        let path = asset_baker::provenance::provenance_path(&textures, &id.atlas_name()).unwrap();
        if !path.is_file() {
            continue;
        }
        let record = asset_baker::provenance::MediaProvenance::load(&path).unwrap();
        let wanted = spec
            .reframe
            .map(|reframe| asset_baker::provenance::ReframeRecord {
                scale_permille: reframe.scale_permille,
                picture_feather_permille: reframe.picture_feather_permille,
                picture_sha256: sha256_file(&id.reframed_idle_picture(&keyframes)).unwrap(),
            });
        assert_eq!(record.bake.reframe, wanted, "{}", id.atlas_name());
        let shape = shipped_catalog().get_by_slug(id.slug).unwrap().shape;
        assert_eq!(
            record.bake.framing,
            spec.bake_framing(
                style.framing.framing_for(id.slug, &shape),
                &shipped_assets().join(MEDIA_STYLE_FILE)
            )
            .unwrap(),
            "{}",
            id.atlas_name()
        );
        if spec.reframe.is_some() {
            // The Enter morph started on exactly that picture.
            let enter = &record.morphs[&Morph::Enter].origin;
            assert!(
                matches!(enter, asset_baker::provenance::ClipOrigin::Veo { keyframe_sha256, .. }
                    if keyframe_sha256.as_deref() == wanted.as_ref().map(|w| w.picture_sha256.as_str())),
                "{enter:?}"
            );
        }
        checked += 1;
    }
    assert!(checked > 0, "the bundle ships mode atlases");
}

#[test]
fn every_shipped_reframed_picture_is_what_reframe_mode_picture_writes() {
    let style = shipped_style();
    let keyframes = shipped_assets().join("ai_keyframes");
    for (id, _) in style
        .modes
        .modes()
        .filter(|(_, spec)| spec.reframe.is_some())
    {
        let dir = tempfile::tempdir().unwrap();
        let idle = format!("{}_idle.png", id.slug);
        std::fs::copy(keyframes.join(&idle), dir.path().join(&idle)).unwrap();
        let output = baker_with_assets(&shipped_assets())
            .args([
                "reframe-mode-picture",
                "--unit",
                id.slug,
                "--mode",
                id.stance,
            ])
            .arg("--keyframes-dir")
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", text(&output));
        let written = image::open(id.reframed_idle_picture(dir.path()))
            .unwrap()
            .to_rgb8();
        let shipped = image::open(id.reframed_idle_picture(&keyframes))
            .expect("a reframed mode ships its reframed Idle picture")
            .to_rgb8();
        assert!(
            written == shipped,
            "{} differs from what reframe-mode-picture writes",
            id.atlas_name()
        );

        // It refuses to overwrite without --replace.
        let again = baker_with_assets(&shipped_assets())
            .args([
                "reframe-mode-picture",
                "--unit",
                id.slug,
                "--mode",
                id.stance,
            ])
            .arg("--keyframes-dir")
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(!again.status.success());
        assert!(text(&again).contains("--replace"), "{}", text(&again));
    }
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

fn mode_rejection(sha256: &str) -> LedgerEvent {
    LedgerEvent::ModeClipRejected {
        slug: "fixture_unit".into(),
        stance: "fixture_mode".into(),
        slot: SliceKey::Morph(Morph::Enter),
        sha256: sha256.into(),
        reason: "superseded".into(),
    }
}

fn mode_withdrawal(sha256: &str) -> LedgerEvent {
    LedgerEvent::ModeClipRejectionWithdrawn {
        slug: "fixture_unit".into(),
        stance: "fixture_mode".into(),
        slot: SliceKey::Morph(Morph::Enter),
        sha256: sha256.into(),
        reason: "the retries were worse; it stays shipped".into(),
    }
}

#[test]
fn a_withdrawn_mode_rejection_restores_the_clip_and_costs_nothing() {
    let enter = SliceKey::Morph(Morph::Enter);
    let mut events = vec![
        mode_started("r/1", enter),
        mode_downloaded("op-1", enter, "aaa"),
        mode_rejection("aaa"),
        mode_started("r/2", enter),
        mode_downloaded("op-2", enter, "bbb"),
        mode_rejection("bbb"),
    ];
    let before =
        LedgerHistory::from_records(&events.iter().cloned().map(record).collect::<Vec<_>>());
    events.push(mode_withdrawal("aaa"));
    let records: Vec<LedgerRecord> = events.iter().cloned().map(record).collect();
    let json = serde_json::to_string(records.last().unwrap()).unwrap();
    assert!(
        json.contains("\"event\":\"mode_clip_rejection_withdrawn\""),
        "{json}"
    );
    assert_eq!(
        &serde_json::from_str::<LedgerRecord>(&json).unwrap(),
        records.last().unwrap()
    );

    let after = LedgerHistory::from_records(&records);
    let clip = after
        .mode_clip("fixture_unit", "fixture_mode", enter)
        .unwrap();
    assert!(!clip.rejected_sha256.contains("aaa"), "withdrawn");
    assert!(
        clip.rejected_sha256.contains("bbb"),
        "the other rejection stands"
    );
    assert_eq!(clip.paid_attempts, 2);
    assert_eq!(clip.unusable_results, 1);
    assert!(clip.downloaded.is_none(), "a withdrawal awaits no review");
    assert_eq!(
        after.paid_start_models().count(),
        before.paid_start_models().count(),
        "a withdrawal is not a paid start"
    );
    let budget = asset_baker::budget::BudgetConfig::load(
        &shipped_assets().join(asset_baker::budget::BUDGET_FILE),
    )
    .unwrap();
    let priced = |history: &LedgerHistory| {
        history
            .paid_start_models()
            .map(|_| {
                budget
                    .price_cents(asset_baker::veo_client::DEFAULT_VEO_MODEL)
                    .unwrap()
            })
            .sum::<u64>()
    };
    assert_eq!(priced(&after), priced(&before));

    // Withdrawing twice changes nothing more; a later rejection applies again.
    events.push(mode_withdrawal("aaa"));
    let twice =
        LedgerHistory::from_records(&events.iter().cloned().map(record).collect::<Vec<_>>());
    assert_eq!(
        twice.mode_clip("fixture_unit", "fixture_mode", enter),
        Some(clip)
    );
    events.push(mode_rejection("aaa"));
    let again = LedgerHistory::from_records(&events.into_iter().map(record).collect::<Vec<_>>());
    assert!(again
        .mode_clip("fixture_unit", "fixture_mode", enter)
        .unwrap()
        .rejected_sha256
        .contains("aaa"));
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

/// A fixture mode atlas baked into `dir` from four installed copies of the fixture clip, with a
/// ledger in `dir` that downloaded each.
struct FixtureAtlas {
    record: asset_baker::provenance::MediaProvenance,
    textures: PathBuf,
    videos: PathBuf,
    ledger_path: PathBuf,
}

/// The fixture atlas's unit and mode.
const FIXTURE_ID: ModeAtlasId<'static> = ModeAtlasId {
    slug: "fixture_unit",
    stance: "fixture_mode",
};

fn bake_fixture_atlas(dir: &Path) -> FixtureAtlas {
    let id = FIXTURE_ID;
    let slots = [
        SliceKey::Action(AnimState::Idle),
        SliceKey::Action(AnimState::Attack),
        SliceKey::Morph(Morph::Enter),
        SliceKey::Morph(Morph::Leave),
    ];
    let videos = dir.join("videos");
    let paths = install_fixture_clips(&videos, id, &slots);
    let sha256 = sha256_file(&paths[0]).unwrap();
    let ledger_path = dir.join("ledger.jsonl");
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
    let textures = dir.join("textures");
    let seams = BTreeMap::new();
    let spec = FrameSpec {
        size: 32,
        fps: 5,
        framing: BakeFraming::CenterCrop,
    };
    let record = bake_mode_with_provenance(
        &id.atlas_name(),
        &clips,
        &videos,
        &textures,
        spec,
        &seams,
        None,
    )
    .expect("the mode atlas bakes");
    FixtureAtlas {
        record,
        textures,
        videos,
        ledger_path,
    }
}

#[test]
fn a_mode_atlas_bakes_its_states_and_morphs_with_their_origins() {
    let dir = tempfile::tempdir().unwrap();
    let id = FIXTURE_ID;
    let FixtureAtlas {
        record, textures, ..
    } = bake_fixture_atlas(dir.path());

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
fn a_shipped_mode_clip_with_a_standing_rejection_fails_the_audit_until_withdrawn() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = bake_fixture_atlas(dir.path());
    let enter = SliceKey::Morph(Morph::Enter);
    let sha256 = fixture.record.morphs[&Morph::Enter].clip_sha256.clone();
    let verify = || {
        let ledger = read_ledger_numbered(&fixture.ledger_path).unwrap();
        verify_mode_provenance(
            FIXTURE_ID.slug,
            FIXTURE_ID.stance,
            &ModeVerifyContext {
                textures_dir: &fixture.textures,
                videos_dir: &fixture.videos,
                ledger: Some(&ledger),
            },
        )
    };
    let append = |event: LedgerEvent| {
        let mut text = std::fs::read_to_string(&fixture.ledger_path).unwrap();
        text.push_str(&serde_json::to_string(&record(event)).unwrap());
        text.push('\n');
        std::fs::write(&fixture.ledger_path, text).unwrap();
    };
    assert_eq!(verify(), Vec::new(), "the fresh bake verifies");

    append(mode_rejection(&sha256));
    let rejected = verify();
    assert!(
        rejected.contains(&UnverifiedReason::ModeClipRejected { slot: enter }),
        "{rejected:?}"
    );

    append(mode_withdrawal(&sha256));
    assert_eq!(verify(), Vec::new(), "a withdrawn rejection verifies again");

    // A later rejection of the same bytes stands again.
    append(mode_rejection(&sha256));
    assert!(verify().contains(&UnverifiedReason::ModeClipRejected { slot: enter }));

    // Without a ledger, nothing can be verified; a changed clip is reported.
    let unsupplied = verify_mode_provenance(
        FIXTURE_ID.slug,
        FIXTURE_ID.stance,
        &ModeVerifyContext {
            textures_dir: &fixture.textures,
            videos_dir: &fixture.videos,
            ledger: None,
        },
    );
    assert_eq!(unsupplied.len(), 4, "{unsupplied:?}");
    std::fs::write(fixture.videos.join(FIXTURE_ID.clip_file(enter)), b"other").unwrap();
    assert!(verify().iter().any(|reason| matches!(
        reason,
        UnverifiedReason::ModeClipMismatch { slot, .. } if *slot == enter
    )));
}

#[test]
fn a_rejection_is_withdrawn_only_where_one_stands() {
    let (slug, stance, _) = shipped_modes().remove(0);
    let dir = tempfile::tempdir().unwrap();
    let clip = dir.path().join("clip.mp4");
    std::fs::write(&clip, b"clip bytes").unwrap();
    let sha256 = sha256_file(&clip).unwrap();
    let ledger = dir.path().join("ledger.jsonl");
    let rejection = LedgerEvent::ModeClipRejected {
        slug: slug.clone(),
        stance: stance.clone(),
        slot: SliceKey::Morph(Morph::Enter),
        sha256: sha256.clone(),
        reason: "superseded".into(),
    };
    let withdraw = || {
        baker_with_assets(&shipped_assets())
            .current_dir(dir.path())
            .args([
                "mode-withdraw-rejection",
                "--unit",
                &slug,
                "--mode",
                &stance,
                "--slice",
                "enter",
                "--reason",
                "the retries were worse",
            ])
            .arg("--clip")
            .arg(&clip)
            .arg("--ledger")
            .arg(&ledger)
            .output()
            .unwrap()
    };
    let refused = withdraw();
    assert!(!refused.status.success());
    assert!(
        text(&refused).contains("no standing rejection"),
        "{}",
        text(&refused)
    );

    std::fs::write(
        &ledger,
        serde_json::to_string(&record(rejection)).unwrap() + "\n",
    )
    .unwrap();
    let output = withdraw();
    assert!(output.status.success(), "{}", text(&output));
    let history = LedgerHistory::load(&ledger).unwrap();
    let clip_history = history
        .mode_clip(&slug, &stance, SliceKey::Morph(Morph::Enter))
        .unwrap();
    assert!(!clip_history.rejected_sha256.contains(&sha256));
    assert_eq!(history.paid_start_models().count(), 0, "it costs nothing");
    assert!(!withdraw().status.success(), "nothing left to withdraw");
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
        None,
    );
    assert!(matches!(
        result,
        Err(asset_baker::provenance::ProvenanceError::SliceOutsideVideos { .. })
    ));
}
