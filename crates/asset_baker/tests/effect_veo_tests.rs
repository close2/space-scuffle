//! Behavioural tests of `asset_baker effect-veo`, the paid Veo runner for effect elements
//! (plan `docs/plans/2026-11-09-effect-visualization.md` §6.2 item 2, `WP6a`).
//!
//! Every request goes through [`common::CurlShim`] to a loopback [`common::FakeServer`], so no
//! test can reach Google, and only obviously fake keys are used. The tests check the rules the
//! unit Veo run follows: the budget is charged before any credential is read, every step is a
//! ledger line under the element's set and name with its model id, a quota refusal is free and
//! exits with status 3, an outstanding operation is resumed instead of paid for again, no
//! `generateAudio` field is sent, and the effect bake finds the clip's origin by its bytes.

#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::budget::{BudgetConfig, BUDGET_FILE};
use asset_baker::ledger::{
    read_ledger_numbered, sha256_file, ApiMode, Ledger, LedgerEvent, LedgerHistory,
};
use asset_baker::provenance::{veo_origin_by_clip_sha256, ClipOrigin};
use asset_baker::VeoParameters;
use common::{CurlShim, FakeServer, Reply};
use serde_json::Value;
use std::path::Path;
use std::process::Output;

mod common;

const MODEL: &str = "veo-3.1-fast-generate-001";

/// `effect-veo` for `fx_test/burst` with the fake Vertex key, the shim and `extra` args.
fn effect_veo(dir: &Path, credentials: &Path, shim: &CurlShim, extra: &[&str]) -> Output {
    common::cli::baker()
        .args([
            "effect-veo",
            "--set",
            "fx_test",
            "--element",
            "burst",
            "--description",
            "A burst of jagged dark spikes erupting upward and retracting.",
        ])
        .args(extra)
        .arg("--ledger")
        .arg(dir.join("ledger.jsonl"))
        .arg("--out-dir")
        .arg(dir.join("clips"))
        .arg("--credentials-dir")
        .arg(credentials)
        .arg("--curl-path")
        .arg(shim.path())
        .output()
        .expect("run baker")
}

fn ledger_lines(dir: &Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join("ledger.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("ledger line"))
        .collect()
}

fn events(lines: &[Value]) -> Vec<&str> {
    lines
        .iter()
        .map(|line| line["event"].as_str().unwrap_or("?"))
        .collect()
}

fn spent_cents(dir: &Path) -> u64 {
    let budget = BudgetConfig::load(&common::cli::shipped_assets().join(BUDGET_FILE))
        .expect("shipped budget");
    let history = LedgerHistory::load(&dir.join("ledger.jsonl")).expect("history");
    budget.spent_cents(&history).expect("priced")
}

#[test]
fn a_clip_is_generated_recorded_under_its_element_and_found_by_the_bake() {
    let clip = std::fs::read(common::fixture_clip()).expect("fixture clip");
    let server = FakeServer::start(common::cli::healthy(clip));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_vertex_credentials(dir.path());

    let output = effect_veo(dir.path(), &credentials, &shim, &["--max-generations", "1"]);
    let text = common::cli::text(&output);
    assert!(output.status.success(), "{text}");
    common::cli::assert_no_key(&text, "the command output");

    let lines = ledger_lines(dir.path());
    assert_eq!(
        events(&lines),
        [
            "effect_clip_request_started",
            "effect_clip_operation_named",
            "effect_clip_operation_done",
            "effect_clip_downloaded"
        ]
    );
    assert_eq!(lines[0]["model"], MODEL);
    assert_eq!(lines[0]["set"], "fx_test");
    assert_eq!(lines[0]["element"], "burst");
    assert!(lines[0]["keyframe_sha256"].is_null());
    assert!(
        lines[0]["parameters"].get("generateAudio").is_none(),
        "sound stays on: no generateAudio field is recorded"
    );
    let negative = lines[0]["parameters"]["negativePrompt"]
        .as_str()
        .expect("negative prompt");
    assert!(negative.contains("horizon"), "{negative}");
    assert!(lines[0]["prompt"]
        .as_str()
        .expect("prompt")
        .contains("pure black"));
    let ledger_text = std::fs::read_to_string(dir.path().join("ledger.jsonl")).expect("ledger");
    common::cli::assert_no_key(&ledger_text, "the ledger");

    let starts: Vec<Value> = server
        .requests()
        .iter()
        .filter(|r| r.path.contains(":predictLongRunning"))
        .map(|r| serde_json::from_slice(&r.body).expect("json body"))
        .collect();
    assert_eq!(starts.len(), 1);
    assert!(starts[0]["parameters"].get("generateAudio").is_none());
    assert!(starts[0]["instances"][0].get("image").is_none());

    let path = lines[3]["path"].as_str().expect("path");
    let sha256 = sha256_file(Path::new(path)).expect("clip");
    assert_eq!(lines[3]["sha256"], sha256.as_str());
    assert!(path.contains("clips/fx_test/burst/"), "{path}");
    assert_eq!(spent_cents(dir.path()), 40);

    let numbered = read_ledger_numbered(&dir.path().join("ledger.jsonl")).expect("ledger");
    match veo_origin_by_clip_sha256(&numbered, &sha256) {
        Some(ClipOrigin::Veo {
            model,
            operation,
            ledger_line,
            prompt_sha256,
            ..
        }) => {
            assert_eq!(model, MODEL);
            assert_eq!(operation, lines[1]["operation"].as_str().expect("op"));
            assert_eq!(ledger_line, 4);
            assert_eq!(
                prompt_sha256,
                lines[0]["prompt_sha256"].as_str().expect("sha")
            );
        }
        other => panic!("the bake must find the effect clip's origin: {other:?}"),
    }
}

#[test]
fn a_keyframe_is_sent_as_the_start_picture_and_recorded() {
    let clip = std::fs::read(common::fixture_clip()).expect("fixture clip");
    let server = FakeServer::start(common::cli::healthy(clip));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_vertex_credentials(dir.path());
    let keyframe = dir.path().join("keyframe.png");
    common::cli::write_picture(&keyframe, 64, 64);

    let keyframe_arg = keyframe.to_str().expect("utf-8 path");
    let output = effect_veo(
        dir.path(),
        &credentials,
        &shim,
        &["--max-generations", "1", "--keyframe", keyframe_arg],
    );
    assert!(output.status.success(), "{}", common::cli::text(&output));
    let lines = ledger_lines(dir.path());
    assert_eq!(
        lines[0]["keyframe_sha256"],
        sha256_file(&keyframe).expect("sha").as_str()
    );
    let start: Value = server
        .requests()
        .iter()
        .find(|r| r.path.contains(":predictLongRunning"))
        .map(|r| serde_json::from_slice(&r.body).expect("json"))
        .expect("a start request");
    assert!(start["instances"][0].get("image").is_some(), "{start}");
    assert!(start["instances"][0].get("lastFrame").is_none(), "{start}");
}

#[test]
fn an_unpriced_model_or_a_missing_cap_is_refused_before_any_credential() {
    let server = FakeServer::start(|_, _| Reply::json("{}"));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let trap = common::cli::trapped_credentials(dir.path());
    for extra in [
        &["--model", "veo-unpriced", "--max-generations", "1"][..],
        &[][..],
    ] {
        let output = effect_veo(dir.path(), &trap, &shim, extra);
        let text = common::cli::text(&output);
        assert!(!output.status.success(), "{text}");
        assert!(
            text.contains("has no price") || text.contains("--max-generations"),
            "{text}"
        );
    }
    assert!(server.requests().is_empty());
    assert!(!dir.path().join("ledger.jsonl").exists());
}

#[test]
fn a_quota_refusal_is_recorded_free_and_exits_with_the_quota_status() {
    let server = FakeServer::start(|_, _| {
        Reply::status(
            429,
            r#"{"error":{"code":429,"message":"Quota exceeded","status":"RESOURCE_EXHAUSTED"}}"#,
        )
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_vertex_credentials(dir.path());

    let output = effect_veo(dir.path(), &credentials, &shim, &["--max-generations", "1"]);
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        common::cli::text(&output)
    );
    let lines = ledger_lines(dir.path());
    assert_eq!(
        events(&lines),
        ["effect_clip_request_started", "effect_clip_quota_refused"]
    );
    assert_eq!(spent_cents(dir.path()), 0);
}

#[test]
fn an_outstanding_operation_is_resumed_and_not_paid_for_again() {
    let clip = std::fs::read(common::fixture_clip()).expect("fixture clip");
    let server = FakeServer::start(common::cli::healthy(clip));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_vertex_credentials(dir.path());
    let mut ledger =
        Ledger::open(&dir.path().join("ledger.jsonl"), "earlier".to_string()).expect("ledger");
    ledger
        .append(LedgerEvent::EffectClipRequestStarted {
            request_id: "earlier/1".to_string(),
            set: "fx_test".to_string(),
            element: "burst".to_string(),
            model: MODEL.to_string(),
            mode: ApiMode::VertexExpress,
            prompt_sha256: "c".repeat(64),
            prompt: "p".to_string(),
            keyframe_sha256: None,
            parameters: VeoParameters::default(),
        })
        .expect("line");
    ledger
        .append(LedgerEvent::EffectClipOperationNamed {
            request_id: "earlier/1".to_string(),
            set: "fx_test".to_string(),
            element: "burst".to_string(),
            model: MODEL.to_string(),
            operation: "projects/p/locations/l/operations/pending-op".to_string(),
        })
        .expect("line");
    drop(ledger);

    let output = effect_veo(dir.path(), &credentials, &shim, &["--max-generations", "1"]);
    let text = common::cli::text(&output);
    assert!(output.status.success(), "{text}");
    assert!(text.contains("Resumed"), "{text}");
    assert!(server
        .requests()
        .iter()
        .all(|r| !r.path.contains(":predictLongRunning")));
    assert_eq!(
        events(&ledger_lines(dir.path())),
        [
            "effect_clip_request_started",
            "effect_clip_operation_named",
            "effect_clip_operation_done",
            "effect_clip_downloaded"
        ]
    );
    assert_eq!(spent_cents(dir.path()), 40, "one start, paid once");
    let history = LedgerHistory::load(&dir.path().join("ledger.jsonl")).expect("history");
    let element = history.effect_clip("fx_test", "burst").expect("history");
    assert!(element.pending.is_none());
    assert!(element.downloaded.is_some());
}
