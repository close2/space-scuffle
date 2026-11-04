//! Behavioural tests of prompt composition and validation over the shipped data (plan
//! `2026-11-04-phase-8-5-veo-media.md`, D-1 and D-2).
//!
//! # Responsibility
//! Proves that prompts are composed from data only (no style text in Rust), that the plan lists
//! exactly the published actions with Idle first, and that validation reports every problem of
//! every paid clip while ignoring clips that cost nothing.
//!
//! # Invariants
//! - Identities are chosen from the shipped catalog by capability, never by name.
//!
//! The loops section covers the loop policy of plan `2026-11-04-veo-seamless-loops.md` (D-1)
//! and the pictures a plan resolves for each state (D-2, D-3).

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::ledger::LedgerHistory;
use asset_baker::prompts::{atlas_loops, KeyframeSource, PromptIssue, MEDIA_STYLE_FILE};
use asset_baker::veo_plan::{published_states, ClipAction};
use asset_baker::{build_action_prompt, plan_run, BakerError, MediaStyle, VeoDirs};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

fn catalog() -> sim_core::UnitCatalog {
    asset_baker::load_catalogs(&assets())
        .expect("catalogs")
        .units
}

fn style() -> MediaStyle {
    MediaStyle::load(&assets()).expect("style")
}

fn dirs(root: &Path) -> VeoDirs {
    VeoDirs {
        keyframes: root.join("keyframes"),
        videos: root.join("videos"),
        clips: root.join("clips"),
    }
}

/// An identity with authored prompts for every state it publishes, and one without any.
fn authored_and_unauthored(catalog: &sim_core::UnitCatalog) -> (String, String) {
    let all_authored = |a: &sim_core::UnitArchetype| {
        catalog
            .publishable_actions(a)
            .iter()
            .all(|state| a.veo_prompt(state).is_some())
    };
    let authored = catalog
        .archetypes()
        .find(|a| all_authored(a))
        .expect("an authored identity");
    let unauthored = catalog
        .archetypes()
        .find(|a| a.veo_prompts.is_empty())
        .expect("an identity without prompts");
    (authored.slug.clone(), unauthored.slug.clone())
}

#[test]
fn prompts_are_composed_from_the_catalog_and_the_style_file() {
    let catalog = catalog();
    let style = style();
    let (authored, unauthored) = authored_and_unauthored(&catalog);
    let archetype = catalog.get_by_slug(&authored).expect("archetype");

    let prompt = build_action_prompt(&catalog, &style, &authored, AnimState::Idle).expect("prompt");

    assert!(prompt.authored);
    assert_eq!(
        prompt.text,
        format!(
            "{} {} {}",
            archetype.veo_prompt(AnimState::Idle).expect("idle"),
            style.actions.idle,
            style.style
        )
    );
    let fallback =
        build_action_prompt(&catalog, &style, &unauthored, AnimState::Idle).expect("prompt");
    assert!(!fallback.authored);
}

#[test]
fn a_style_edit_changes_every_prompt_without_a_rebuild() {
    let catalog = catalog();
    let mut style = style();
    let (authored, _) = authored_and_unauthored(&catalog);
    style.style = "Flat watercolour look.".to_string();

    let prompt = build_action_prompt(&catalog, &style, &authored, AnimState::Walk).expect("prompt");

    assert!(
        prompt.text.ends_with("Flat watercolour look."),
        "{}",
        prompt.text
    );
}

#[test]
fn published_states_put_idle_first_and_refuse_unpublished_filters() {
    let catalog = catalog();
    for archetype in catalog.archetypes() {
        let states = published_states(&catalog, &archetype.slug, None).expect("states");
        assert_eq!(states.first(), Some(&AnimState::Idle), "{}", archetype.slug);
        assert_eq!(
            states.len(),
            catalog.publishable_actions(archetype).iter().count()
        );
        assert!(!states.contains(&AnimState::Die), "Die is never published");
    }
    let immobile = catalog
        .archetypes()
        .find(|a| !a.can_ever_move())
        .expect("an immobile identity");
    assert!(matches!(
        published_states(&catalog, &immobile.slug, Some(&[AnimState::Walk])),
        Err(BakerError::StateNotPublished { .. })
    ));
}

#[test]
fn validation_reports_every_problem_of_paid_clips_only() {
    let catalog = catalog();
    let style = style();
    let (authored, unauthored) = authored_and_unauthored(&catalog);
    let root = tempfile::tempdir().expect("tempdir");
    let dirs = dirs(root.path());
    std::fs::create_dir_all(&dirs.videos).expect("videos");
    // The unauthored identity's Idle is installed, so only its other states are paid for.
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/luma_ramp_4s.mp4"),
        dirs.installed_clip(&unauthored, AnimState::Idle),
    )
    .expect("install");

    let plan = plan_run(
        &catalog,
        &style,
        &[authored.clone(), unauthored.clone()],
        None,
        &dirs,
        &LedgerHistory::default(),
    )
    .expect("plan");
    let problems = plan.problems(&style);

    assert!(
        problems.iter().all(|p| p.slug == unauthored),
        "{problems:?}"
    );
    let expected: Vec<AnimState> = published_states(&catalog, &unauthored, None)
        .expect("states")
        .into_iter()
        .filter(|state| *state != AnimState::Idle)
        .collect();
    let reported: Vec<AnimState> = problems.iter().map(|p| p.state).collect();
    assert_eq!(reported, expected);
    assert!(problems.iter().all(|p| p.issue == PromptIssue::NotAuthored));
    let idle = plan
        .clips
        .iter()
        .find(|c| c.slug == unauthored && c.state == AnimState::Idle)
        .expect("idle clip");
    assert!(matches!(idle.action, ClipAction::Reuse(_)));
    assert!(plan
        .clips
        .iter()
        .filter(|c| c.slug == unauthored && c.state != AnimState::Idle)
        .all(|c| c.keyframe == KeyframeSource::ChainedFromIdle));
}

// --- Loops ----------------------------------------------------------------------------------

/// The shipped media style file as text.
fn style_text() -> String {
    std::fs::read_to_string(assets().join(MEDIA_STYLE_FILE)).expect("style text")
}

/// Loads the style text `edit` makes of the shipped one, checked against the shipped catalog.
fn load_edited(edit: impl Fn(String) -> String) -> Result<MediaStyle, BakerError> {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(MEDIA_STYLE_FILE);
    std::fs::create_dir_all(path.parent().expect("catalog dir")).expect("catalog dir");
    std::fs::write(&path, edit(style_text())).expect("write style");
    MediaStyle::load_for_catalog(dir.path(), &catalog())
}

/// Replaces the shipped `loops.states` entry of `state` (one line) with `entry`.
fn with_state_entry(text: &str, state: &str, entry: Option<&str>) -> String {
    let start = text
        .find(&format!("            {state}: ("))
        .unwrap_or_else(|| panic!("the shipped {state} entry"));
    let end = start + text[start..].find('\n').expect("end of line") + 1;
    let replacement = entry.map_or(String::new(), |entry| format!("            {entry}\n"));
    format!("{}{replacement}{}", &text[..start], &text[end..])
}

/// Replaces the loop policy's (last) `overrides: {}` with `overrides`.
fn with_loop_overrides(text: &str, overrides: &str) -> String {
    let at = text.rfind("overrides: {}").expect("the loop overrides");
    format!(
        "{}overrides: {overrides}{}",
        &text[..at],
        &text[at + "overrides: {}".len()..]
    )
}

#[test]
fn loops_the_shipped_policy_pins_every_looping_state_and_constructing() {
    let style = MediaStyle::load_for_catalog(&assets(), &catalog()).expect("shipped style");
    for state in AnimState::ALL {
        let spec = style.loops.spec_for("any_identity", state);
        if atlas_loops(state) {
            let spec = spec.expect("a looping state has an entry");
            assert!(spec.pin_end, "{state:?}");
            assert!(spec.seam.is_some(), "{state:?}");
        }
    }
    assert!(style
        .loops
        .pins_end("any_identity", AnimState::Constructing));
    assert!(!style.loops.pins_end("any_identity", AnimState::Attack));
    assert!(!style.loops.pins_end("any_identity", AnimState::Die));
}

#[test]
fn loops_invalid_policies_are_refused_at_load() {
    let data_error = |result: Result<MediaStyle, BakerError>, needle: &str| match result {
        Err(BakerError::DataFile { reason, .. }) => {
            assert!(reason.contains(needle), "{reason}");
        }
        other => panic!("expected a data file error naming '{needle}', got {other:?}"),
    };

    let seam_on_attack = load_edited(|text| {
        with_state_entry(
            &text,
            "Constructing",
            Some("Attack: (pin_end: false, seam: Some((scale_lock: true, crossfade_frames: 2))),"),
        )
    });
    data_error(seam_on_attack, "does not loop Attack");

    let no_idle = load_edited(|text| with_state_entry(&text, "Idle", None));
    data_error(no_idle, "no entry for Idle");

    let long_fade =
        load_edited(|text| text.replacen("crossfade_frames: 2", "crossfade_frames: 5", 1));
    data_error(long_fade, "crossfade_frames 5");

    let unknown = load_edited(|text| {
        with_loop_overrides(
            &text,
            "{ \"no_such_identity\": { Idle: (pin_end: false) } }",
        )
    });
    assert!(
        matches!(unknown, Err(BakerError::UnknownLoopOverride { ref slug, .. }) if slug == "no_such_identity"),
        "{unknown:?}"
    );

    // An identity publishing four states keeps 8 frames per state: a quarter is 2.
    let catalog = catalog();
    let four = catalog
        .archetypes()
        .find(|a| catalog.publishable_actions(a).iter().count() == 4)
        .expect("an identity publishing four states");
    let over_budget = load_edited(|text| {
        with_loop_overrides(
            &text,
            &format!(
                "{{ \"{}\": {{ Idle: (pin_end: true, seam: Some((scale_lock: true, crossfade_frames: 3))) }} }}",
                four.slug
            ),
        )
    });
    data_error(over_budget, "more than a quarter of the 8 frames");
}

#[test]
fn loops_an_override_changes_only_its_identity_and_state() {
    let catalog = catalog();
    let (authored, other) = authored_and_unauthored(&catalog);
    let style = load_edited(|text| {
        with_loop_overrides(
            &text,
            &format!("{{ \"{authored}\": {{ Idle: (pin_end: false, seam: None) }} }}"),
        )
    })
    .expect("a valid override");

    assert!(!style.loops.pins_end(&authored, AnimState::Idle));
    assert!(style.loops.pins_end(&authored, AnimState::Constructing));
    assert!(style.loops.pins_end(&other, AnimState::Idle));
}

#[test]
fn loops_atlas_loops_is_what_the_bake_writes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let clip = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/luma_ramp_4s.mp4");
    let clips: Vec<(AnimState, PathBuf)> = AnimState::ALL
        .into_iter()
        .map(|state| (state, clip.clone()))
        .collect();

    let meta =
        asset_baker::bake_multi_action_videos_with_states("loop_probe", &clips, dir.path(), 32, 15)
            .expect("bake every state");

    for state in AnimState::ALL {
        let slice = meta.actions.get(&state).expect("baked state");
        assert_eq!(slice.looping, atlas_loops(state), "{state:?}");
    }
}

#[test]
fn loops_chained_states_share_a_valid_idle_pin_and_ignore_a_square_one() {
    let catalog = catalog();
    let style = style();
    let (authored, _) = authored_and_unauthored(&catalog);
    let root = tempfile::tempdir().expect("tempdir");
    let dirs = dirs(root.path());
    std::fs::create_dir_all(&dirs.keyframes).expect("keyframes");
    let pin = dirs.keyframes.join(format!("{authored}_idle.png"));
    let plan_others = || {
        plan_run(
            &catalog,
            &style,
            std::slice::from_ref(&authored),
            None,
            &dirs,
            &LedgerHistory::default(),
        )
        .expect("plan")
    };

    image::RgbImage::new(1280, 720).save(&pin).expect("pin");
    let plan = plan_others();
    for clip in &plan.clips {
        let expected = if clip.state == AnimState::Idle {
            KeyframeSource::File(pin.clone())
        } else {
            KeyframeSource::IdlePin(pin.clone())
        };
        assert_eq!(clip.keyframe, expected, "{:?}", clip.state);
        assert_eq!(clip.pin_end, style.loops.pins_end(&authored, clip.state));
    }
    assert!(
        plan.problems(&style).is_empty(),
        "{:?}",
        plan.problems(&style)
    );

    image::RgbImage::new(1024, 1024).save(&pin).expect("square");
    let plan = plan_others();
    for clip in plan.clips.iter().filter(|c| c.state != AnimState::Idle) {
        assert_eq!(
            clip.keyframe,
            KeyframeSource::ChainedFromIdle,
            "{:?}",
            clip.state
        );
    }
    let problems = plan.problems(&style);
    assert!(
        problems
            .iter()
            .any(|p| p.state == AnimState::Idle && matches!(p.issue, PromptIssue::PinShape { .. })),
        "{problems:?}"
    );
}

#[test]
fn a_chained_state_without_any_idle_clip_is_a_problem() {
    let catalog = catalog();
    let style = style();
    let (authored, _) = authored_and_unauthored(&catalog);
    let root = tempfile::tempdir().expect("tempdir");
    let states = published_states(&catalog, &authored, None).expect("states");
    let other = states[1];

    let plan = plan_run(
        &catalog,
        &style,
        std::slice::from_ref(&authored),
        Some(&[other]),
        &dirs(root.path()),
        &LedgerHistory::default(),
    )
    .expect("plan");

    let problems = plan.problems(&style);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert_eq!(problems[0].issue, PromptIssue::NoIdleToChain);
}
