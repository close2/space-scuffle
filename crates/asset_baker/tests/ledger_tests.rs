//! Behavioural tests of the Veo ledger through its public API (plan
//! `2026-11-04-phase-8-5-veo-media.md`, D-3 and D-4).
//!
//! # Responsibility
//! Proves that the ledger round-trips every event in order, that its history decides resume,
//! review and regeneration correctly, that the rolling budget excludes quota refusals and old
//! lines, and that a damaged ledger is refused with its line number.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::ledger::{
    format_utc, new_run_id, read_ledger, ApiMode, Ledger, LedgerError, LedgerEvent, LedgerHistory,
    OperationOutcome,
};
use asset_baker::VeoParameters;
use sim_types::enums::AnimState;

const SLUG: &str = "fixture_identity";

fn started(request_id: &str, state: AnimState) -> LedgerEvent {
    LedgerEvent::RequestStarted {
        request_id: request_id.to_string(),
        slug: SLUG.to_string(),
        state,
        model: "fixture-model".to_string(),
        mode: ApiMode::GeminiApi,
        prompt_sha256: "0".repeat(64),
        prompt: "a prompt".to_string(),
        keyframe_sha256: None,
        last_frame_sha256: None,
        parameters: VeoParameters::default(),
    }
}

fn named(request_id: &str, state: AnimState, operation: &str) -> LedgerEvent {
    LedgerEvent::OperationNamed {
        request_id: request_id.to_string(),
        slug: SLUG.to_string(),
        state,
        model: "fixture-model".to_string(),
        operation: operation.to_string(),
    }
}

fn done(state: AnimState, operation: &str, outcome: OperationOutcome) -> LedgerEvent {
    LedgerEvent::OperationDone {
        slug: SLUG.to_string(),
        state,
        operation: operation.to_string(),
        outcome,
    }
}

fn downloaded(state: AnimState, operation: &str, sha: &str, path: &str) -> LedgerEvent {
    LedgerEvent::Downloaded {
        slug: SLUG.to_string(),
        state,
        operation: operation.to_string(),
        bytes: 10,
        sha256: sha.to_string(),
        duration_seconds: 4.0,
        frames: 96,
        width: 1280,
        height: 720,
        path: path.to_string(),
    }
}

/// Writes `events` to a fresh ledger and returns the directory holding it.
fn ledger_with(events: Vec<LedgerEvent>) -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("nested").join("ledger.jsonl");
    let mut ledger = Ledger::open(&path, new_run_id()).expect("open");
    for event in events {
        ledger.append(event).expect("append");
    }
    (dir, path)
}

#[test]
fn every_event_round_trips_in_order() {
    let events = vec![
        started("r/1", AnimState::Idle),
        named("r/1", AnimState::Idle, "operations/a"),
        done(
            AnimState::Idle,
            "operations/a",
            OperationOutcome::VideoInline,
        ),
        downloaded(AnimState::Idle, "operations/a", "aa", "/clips/a.mp4"),
        LedgerEvent::QuotaRefused {
            request_id: "r/2".to_string(),
            slug: SLUG.to_string(),
            state: AnimState::Walk,
            http_status: 429,
            status: Some("RESOURCE_EXHAUSTED".to_string()),
            reason: None,
        },
        LedgerEvent::StartFailed {
            request_id: "r/3".to_string(),
            slug: SLUG.to_string(),
            state: AnimState::Cast,
            error: "HTTP 500".to_string(),
        },
        LedgerEvent::RejectedInReview {
            slug: SLUG.to_string(),
            state: AnimState::Idle,
            sha256: "aa".to_string(),
            reason: "morphs into another object".to_string(),
        },
    ];
    let (_dir, path) = ledger_with(events.clone());

    let records = read_ledger(&path).expect("read");

    let read: Vec<LedgerEvent> = records.iter().map(|r| r.event.clone()).collect();
    assert_eq!(read, events);
    assert!(records.iter().all(|r| r.time_utc.ends_with('Z')));
    let text = std::fs::read_to_string(&path).expect("text");
    assert_eq!(text.lines().count(), 7, "one line per event");
    assert!(text.contains(r#""state":"idle""#), "{text}");
}

#[test]
fn a_last_frame_hash_is_written_only_when_an_end_frame_was_sent() {
    let pinned = LedgerEvent::RequestStarted {
        request_id: "r/1".to_string(),
        slug: SLUG.to_string(),
        state: AnimState::Idle,
        model: "fixture-model".to_string(),
        mode: ApiMode::VertexExpress,
        prompt_sha256: "0".repeat(64),
        prompt: "a prompt".to_string(),
        keyframe_sha256: Some("a".repeat(64)),
        last_frame_sha256: Some("a".repeat(64)),
        parameters: VeoParameters::default(),
    };
    let (_dir, path) = ledger_with(vec![pinned.clone(), started("r/2", AnimState::Attack)]);

    let records = read_ledger(&path).expect("read");

    assert_eq!(records[0].event, pinned);
    let text = std::fs::read_to_string(&path).expect("text");
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines[0].contains(&format!(r#""last_frame_sha256":"{}""#, "a".repeat(64))));
    assert!(
        !lines[1].contains("last_frame_sha256"),
        "an absent end frame writes no field: {}",
        lines[1]
    );
}

#[test]
fn every_committed_ledger_line_still_parses_and_round_trips_unchanged() {
    // The first 25 lines predate `lastFrame` (seamless-loops plan WP1); the pilot of WP4 and
    // later runs send the pin as both the start and the end picture (plan D-2).
    const LINES_BEFORE_LAST_FRAME: usize = 25;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/evidence/veo/ledger.jsonl");
    let text = std::fs::read_to_string(&path).expect("committed ledger");
    let records = read_ledger(&path).expect("every committed line parses");
    let lines: Vec<&str> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    assert_eq!(records.len(), lines.len());
    assert_ne!(records.len(), 0, "the committed ledger has lines");
    for (index, (record, line)) in records.iter().zip(lines).enumerate() {
        if let LedgerEvent::RequestStarted {
            keyframe_sha256,
            last_frame_sha256,
            ..
        } = &record.event
        {
            if index < LINES_BEFORE_LAST_FRAME {
                assert_eq!(
                    *last_frame_sha256,
                    None,
                    "line {}: no request before the pin sent an end frame",
                    index + 1
                );
            } else if last_frame_sha256.is_some() {
                assert_eq!(
                    last_frame_sha256,
                    keyframe_sha256,
                    "line {}: an end frame is the start picture (the pin)",
                    index + 1
                );
            }
        }
        let original: serde_json::Value = serde_json::from_str(line).expect("JSON");
        let written = serde_json::to_value(record).expect("encode");
        assert_eq!(
            written, original,
            "a committed line must keep its exact record"
        );
    }
}

#[test]
fn history_decides_resume_review_and_regeneration() {
    let (_dir, path) = ledger_with(vec![
        // Idle: named, never finished: resumable.
        started("r/1", AnimState::Idle),
        named("r/1", AnimState::Idle, "operations/idle"),
        // Walk: downloaded: awaiting review.
        started("r/2", AnimState::Walk),
        named("r/2", AnimState::Walk, "operations/walk"),
        done(
            AnimState::Walk,
            "operations/walk",
            OperationOutcome::VideoInline,
        ),
        downloaded(AnimState::Walk, "operations/walk", "bb", "/clips/walk.mp4"),
        // Attack: failed with code 13.
        started("r/3", AnimState::Attack),
        named("r/3", AnimState::Attack, "operations/attack"),
        done(
            AnimState::Attack,
            "operations/attack",
            OperationOutcome::Failed {
                code: Some(13),
                message: "internal".to_string(),
            },
        ),
    ]);

    let history = LedgerHistory::load(&path).expect("history");

    let idle = history.clip(SLUG, AnimState::Idle).expect("idle");
    assert_eq!(
        idle.pending.as_ref().map(|p| p.operation.as_str()),
        Some("operations/idle")
    );
    let walk = history.clip(SLUG, AnimState::Walk).expect("walk");
    assert_eq!(walk.pending, None);
    assert_eq!(
        walk.downloaded.as_ref().map(|d| d.sha256.as_str()),
        Some("bb")
    );
    let attack = history.clip(SLUG, AnimState::Attack).expect("attack");
    assert_eq!(attack.unusable_results, 1);
    assert_eq!(attack.pending, None);
    assert!(history.knows_failure("failed:13"));
    assert!(!history.knows_failure("failed:none"));
    assert_eq!(history.clip(SLUG, AnimState::Cast), None);
}

#[test]
fn a_rejection_clears_the_download_and_counts_as_unusable() {
    let (_dir, path) = ledger_with(vec![
        started("r/1", AnimState::Idle),
        named("r/1", AnimState::Idle, "operations/a"),
        done(
            AnimState::Idle,
            "operations/a",
            OperationOutcome::VideoInline,
        ),
        downloaded(AnimState::Idle, "operations/a", "aa", "/clips/a.mp4"),
        LedgerEvent::RejectedInReview {
            slug: SLUG.to_string(),
            state: AnimState::Idle,
            sha256: "aa".to_string(),
            reason: "text in frame".to_string(),
        },
    ]);

    let history = LedgerHistory::load(&path).expect("history");

    let idle = history.clip(SLUG, AnimState::Idle).expect("idle");
    assert_eq!(idle.downloaded, None);
    assert_eq!(idle.unusable_results, 1);
    assert!(idle.rejected_sha256.contains("aa"));
}

#[test]
fn the_window_counts_paid_starts_only() {
    let (_dir, path) = ledger_with(vec![
        started("r/1", AnimState::Idle),
        started("r/2", AnimState::Walk),
        LedgerEvent::QuotaRefused {
            request_id: "r/2".to_string(),
            slug: SLUG.to_string(),
            state: AnimState::Walk,
            http_status: 429,
            status: None,
            reason: None,
        },
    ]);
    let history = LedgerHistory::load(&path).expect("history");

    assert_eq!(
        history.generations_since(0),
        1,
        "a quota refusal costs nothing"
    );
    assert_eq!(
        history.generations_since(u64::MAX),
        0,
        "nothing in a future window"
    );
    assert_eq!(
        history
            .clip(SLUG, AnimState::Walk)
            .expect("walk")
            .paid_attempts,
        0
    );
}

#[test]
fn a_missing_ledger_is_empty_and_a_damaged_one_names_its_line() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("absent.jsonl");
    assert_eq!(read_ledger(&missing).expect("empty").len(), 0);

    let (_keep, path) = ledger_with(vec![started("r/1", AnimState::Idle)]);
    let mut text = std::fs::read_to_string(&path).expect("text");
    text.push_str("\n{\"event\":\"request_started\"}\n");
    std::fs::write(&path, text).expect("write");

    match read_ledger(&path) {
        Err(LedgerError::Malformed { line, .. }) => assert_eq!(line, 3),
        other => panic!("expected a malformed line, got {other:?}"),
    }
}

#[test]
fn run_ids_and_times_are_utc() {
    assert_eq!(format_utc(86_399), "1970-01-01T23:59:59Z");
    let id = new_run_id();
    assert!(id.contains('Z'), "{id}");
    assert!(id.ends_with(&format!("-{}", std::process::id())), "{id}");
}
