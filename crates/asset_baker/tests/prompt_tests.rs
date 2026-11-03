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

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::ledger::LedgerHistory;
use asset_baker::prompts::{KeyframeSource, PromptIssue};
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
