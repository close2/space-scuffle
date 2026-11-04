//! Behavioural tests of the paid Veo pipeline through the built `asset_baker` binary.
//!
//! # Responsibility
//! Proves the spending rules of plan `2026-11-04-phase-8-5-veo-media.md` (WP1) end to end:
//! exactly the published actions are generated (D-1), prompts are validated before credentials
//! are read (D-2), every request is written to the ledger and the key never is (D-3), the caps
//! and the quota stop hold (D-4), the free preflight (D-5), and keyframe chaining, review and
//! installation (D-7).
//!
//! # How it fits
//! The binary runs with the real curl behind [`common::CurlShim`], aimed at a loopback
//! [`common::FakeServer`], and with a fake key in a temporary directory. No test reaches a
//! remote service, and no test reads a real key: the key variables are removed from the child's
//! environment and `--credentials-dir` always names a temporary directory.
//!
//! # Invariants
//! - Identities are picked from the shipped catalog by capability, never by name.

#![cfg(unix)]
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::cast_precision_loss
)]

use base64::prelude::*;
use common::cli::{
    another_combat_unit, assert_no_key, baker, baker_with_assets, combat_unit, healthy,
    identity_with_actions, inline_done, shipped_assets, shipped_catalog, starts, state_of_body,
    text, trapped_credentials, RunDirs, FAKE_KEY,
};
use common::{CurlShim, FakeServer, Reply};
use serde_json::{json, Value};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};

mod common;

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(common::fixture_clip()).expect("read fixture clip")
}

/// A server that must not be called.
fn silent_server() -> FakeServer {
    FakeServer::start(|_, _| Reply::status(500, "must not be called"))
}

/// An immobile structure some builder raises: it publishes Idle and Constructing.
fn structure(catalog: &sim_core::UnitCatalog) -> String {
    identity_with_actions(
        catalog,
        &[AnimState::Idle, AnimState::Constructing],
        false,
        |_| true,
    )
}

/// An armed harvester: Idle, Walk, Attack and Cast.
fn harvester(catalog: &sim_core::UnitCatalog) -> String {
    identity_with_actions(
        catalog,
        &[
            AnimState::Idle,
            AnimState::Walk,
            AnimState::Attack,
            AnimState::Cast,
        ],
        false,
        |archetype| archetype.capabilities.harvester.is_some(),
    )
}

/// A mobile unit with no weapon that neither harvests nor builds: Idle and Walk.
fn unarmed(catalog: &sim_core::UnitCatalog) -> String {
    identity_with_actions(catalog, &[AnimState::Idle, AnimState::Walk], false, |_| {
        true
    })
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Copies the shipped catalogs and media data into a temporary bundle, with the media style
/// rewritten by `edit`.
fn bundle_with_style(edit: impl Fn(String) -> String) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for sub in ["catalog", "media"] {
        let from = shipped_assets().join(sub);
        std::fs::create_dir_all(dir.path().join(sub)).expect("bundle dir");
        for entry in std::fs::read_dir(&from).expect("shipped dir") {
            let entry = entry.expect("entry");
            std::fs::copy(entry.path(), dir.path().join(sub).join(entry.file_name()))
                .expect("copy data file");
        }
    }
    let style = dir.path().join("catalog/media_style.ron");
    let edited = edit(std::fs::read_to_string(&style).expect("style"));
    std::fs::write(&style, edited).expect("write style");
    dir
}

/// Runs a full healthy fake generation of `unit` and returns the run's directories.
fn generated(unit: &str, states: &str) -> (RunDirs, FakeServer, CurlShim, std::process::Output) {
    let server = FakeServer::start(healthy(fixture_bytes()));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    let output = dirs.generate(
        &shim,
        &[unit],
        &["--states", states, "--max-generations", "3"],
    );
    (dirs, server, shim, output)
}

// --- D-1: exactly the published actions -----------------------------------------------------

#[test]
fn d1_the_clip_list_is_exactly_the_published_actions() {
    let catalog = shipped_catalog();
    let cases = [
        (structure(&catalog), vec!["idle", "constructing"]),
        (harvester(&catalog), vec!["idle", "walk", "attack", "cast"]),
        (unarmed(&catalog), vec!["idle", "walk"]),
    ];
    let dir = tempfile::tempdir().expect("tempdir");
    let trap = trapped_credentials(dir.path());
    for (slug, expected) in cases {
        let output = baker()
            .args(["generate-veo", "--dry-run", "--unit", &slug])
            .arg("--keyframes-dir")
            .arg(dir.path().join("none"))
            .arg("--videos-dir")
            .arg(dir.path().join("videos"))
            .arg("--clips-dir")
            .arg(dir.path().join("clips"))
            .arg("--ledger")
            .arg(dir.path().join("ledger.jsonl"))
            .arg("--credentials-dir")
            .arg(&trap)
            .output()
            .expect("run baker");
        let listed: Vec<String> = stdout(&output)
            .lines()
            .filter_map(|line| line.strip_prefix(&format!("clip {slug} ")))
            .map(|rest| rest.split(':').next().unwrap_or("").to_string())
            .collect();
        assert_eq!(listed, expected, "{slug}: {}", text(&output));
        assert!(
            stdout(&output).contains(&format!("Clips to generate: {}", expected.len())),
            "{}",
            text(&output)
        );
    }
}

#[test]
fn d1_existing_clips_are_reused_and_the_dry_run_reads_no_credentials() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = silent_server();
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    std::fs::copy(
        common::fixture_clip(),
        dirs.videos.join(format!("{unit}_idle.mp4")),
    )
    .expect("install idle clip");
    let trap = trapped_credentials(dirs.root.path());
    let run = |extra: &[&str]| {
        baker()
            .arg("generate-veo")
            .args(["--unit", &unit])
            .arg("--keyframes-dir")
            .arg(&dirs.keyframes)
            .arg("--videos-dir")
            .arg(&dirs.videos)
            .arg("--clips-dir")
            .arg(&dirs.clips)
            .arg("--ledger")
            .arg(&dirs.ledger)
            .arg("--credentials-dir")
            .arg(&trap)
            .arg("--curl-path")
            .arg(shim.path())
            .args(extra)
            .output()
            .expect("run baker")
    };

    let dry = run(&["--dry-run"]);
    assert!(dry.status.success(), "{}", text(&dry));
    let out = stdout(&dry);
    assert!(out.contains(&format!("clip {unit} idle: reuse")), "{out}");
    assert!(
        out.contains(&format!("clip {unit} walk: generate")),
        "{out}"
    );
    assert!(
        out.contains(&format!("clip {unit} attack: generate")),
        "{out}"
    );
    assert!(out.contains("Clips to generate: 2"), "{out}");

    // Control: the same run for real does try to read the trapped credentials, and fails.
    let paid = run(&["--max-generations", "2"]);
    assert!(!paid.status.success(), "{}", text(&paid));
    assert!(
        String::from_utf8_lossy(&paid.stderr).contains(".env"),
        "{}",
        text(&paid)
    );
    assert!(server.requests().is_empty(), "no request may be made");
}

#[test]
fn d1_a_states_filter_naming_an_unpublished_state_is_refused() {
    let catalog = shipped_catalog();
    let slug = structure(&catalog);
    let dirs = RunDirs::new();
    let server = silent_server();
    let shim = CurlShim::new(&server);

    let output = dirs.generate(&shim, &[&slug], &["--states", "walk", "--dry-run"]);

    assert!(!output.status.success(), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not publish"),
        "{}",
        text(&output)
    );
}

#[test]
fn d1_bake_unit_videos_refuses_a_state_the_identity_does_not_publish() {
    let catalog = shipped_catalog();
    let slug = structure(&catalog);
    let dir = tempfile::tempdir().expect("tempdir");
    let clip = common::fixture_clip();

    let output = baker()
        .args([
            "bake-unit-videos",
            "--unit",
            &slug,
            "--size",
            "16",
            "--fps",
            "2",
        ])
        .arg("--idle")
        .arg(&clip)
        .arg("--walk")
        .arg(&clip)
        .arg("--output-dir")
        .arg(dir.path())
        .output()
        .expect("run baker");

    assert!(!output.status.success(), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("does not publish"),
        "{}",
        text(&output)
    );
    assert!(!dir.path().join(format!("{slug}.png")).exists());
}

// --- D-2: prompt validation before any spend -------------------------------------------------

#[test]
fn d2_a_missing_prompt_is_refused_before_credentials_are_read() {
    let catalog = shipped_catalog();
    let slug = structure(&catalog);
    let dirs = RunDirs::new();
    let trap = trapped_credentials(dirs.root.path());
    let server = silent_server();
    let shim = CurlShim::new(&server);

    let validate = baker()
        .args(["validate-prompts", "--unit", &slug])
        .arg("--keyframes-dir")
        .arg(&dirs.keyframes)
        .arg("--videos-dir")
        .arg(&dirs.videos)
        .output()
        .expect("run baker");
    assert!(!validate.status.success(), "{}", text(&validate));
    let report = String::from_utf8_lossy(&validate.stderr).into_owned();
    assert!(report.contains(&format!("{slug} idle")), "{report}");
    assert!(report.contains(&format!("{slug} constructing")), "{report}");
    assert!(report.contains("no authored prompt"), "{report}");

    let paid = baker()
        .args(["generate-veo", "--unit", &slug, "--max-generations", "2"])
        .arg("--keyframes-dir")
        .arg(&dirs.keyframes)
        .arg("--videos-dir")
        .arg(&dirs.videos)
        .arg("--clips-dir")
        .arg(&dirs.clips)
        .arg("--ledger")
        .arg(&dirs.ledger)
        .arg("--credentials-dir")
        .arg(&trap)
        .arg("--curl-path")
        .arg(shim.path())
        .output()
        .expect("run baker");
    assert!(!paid.status.success());
    let stderr = String::from_utf8_lossy(&paid.stderr);
    assert!(stderr.contains("no authored prompt"), "{}", text(&paid));
    assert!(
        !stderr.contains(".env"),
        "credentials were read: {}",
        text(&paid)
    );
    assert!(server.requests().is_empty());
    assert!(!dirs.ledger.exists(), "nothing may be recorded");
}

/// Runs `validate-prompts` for `unit` against `bundle` and returns its stderr.
fn validation_errors(bundle: &Path, unit: &str, keyframes: &Path) -> String {
    let output = baker_with_assets(bundle)
        .args(["validate-prompts", "--unit", unit])
        .arg("--keyframes-dir")
        .arg(keyframes)
        .arg("--videos-dir")
        .arg(keyframes.join("no-videos"))
        .output()
        .expect("run baker");
    assert!(!output.status.success(), "{}", text(&output));
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn d2_long_placeholder_and_denied_prompts_are_refused() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let archetype = catalog.get_by_slug(&unit).expect("unit");
    let idle_prompt = archetype.veo_prompt(AnimState::Idle).expect("idle prompt");
    let word = idle_prompt
        .split(|c: char| !c.is_ascii_alphabetic())
        .find(|word| word.len() >= 5)
        .expect("a long word in the prompt")
        .to_string();
    let keyframes = tempfile::tempdir().expect("tempdir");

    let short = bundle_with_style(|s| s.replace("max_prompt_chars: 1024", "max_prompt_chars: 40"));
    let report = validation_errors(short.path(), &unit, keyframes.path());
    assert!(report.contains("longer than 40"), "{report}");

    let placeholder = bundle_with_style(|s| {
        s.replace(
            "placeholder_markers: [",
            &format!("placeholder_markers: [\"{word}\", "),
        )
    });
    let report = validation_errors(placeholder.path(), &unit, keyframes.path());
    assert!(report.contains("placeholder"), "{report}");
    assert!(report.contains(&word), "{report}");

    let denied =
        bundle_with_style(|s| s.replace("deny_list: [", &format!("deny_list: [\"{word}\", ")));
    let report = validation_errors(denied.path(), &unit, keyframes.path());
    assert!(report.contains("denied"), "{report}");

    // The shipped style itself passes, so each refusal above is due to the edit.
    let output = baker()
        .args(["validate-prompts", "--unit", &unit])
        .arg("--keyframes-dir")
        .arg(keyframes.path())
        .arg("--videos-dir")
        .arg(keyframes.path().join("no-videos"))
        .output()
        .expect("run baker");
    assert!(output.status.success(), "{}", text(&output));
}

#[test]
fn d2_an_unreadable_keyframe_is_refused() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let keyframes = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        keyframes.path().join(format!("{unit}_idle.jpg")),
        b"not an image",
    )
    .expect("keyframe");

    let report = validation_errors(&shipped_assets(), &unit, keyframes.path());

    assert!(report.contains("keyframe"), "{report}");
    assert!(report.contains(&format!("{unit}_idle.jpg")), "{report}");
}

// --- D-3: the ledger, and the key never written ----------------------------------------------

#[test]
fn d3_every_event_is_recorded_in_order_and_the_key_never_is() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let (dirs, server, shim, output) = generated(&unit, "idle,walk,attack");

    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(starts(&server), 3);
    let per_clip = [
        "request_started",
        "operation_named",
        "operation_done",
        "downloaded",
    ];
    let expected: Vec<String> = per_clip
        .iter()
        .cycle()
        .take(12)
        .map(ToString::to_string)
        .collect();
    assert_eq!(dirs.ledger_events(), expected);

    let lines = dirs.ledger_lines();
    let states: Vec<&str> = lines
        .iter()
        .filter(|line| line["event"] == "request_started")
        .map(|line| line["state"].as_str().expect("state"))
        .collect();
    assert_eq!(states, ["idle", "walk", "attack"]);
    for line in &lines {
        assert_eq!(line["slug"], unit.as_str());
        assert!(line["time_utc"].as_str().expect("time").ends_with('Z'));
    }
    let started = &lines[0];
    assert_eq!(started["prompt_sha256"].as_str().expect("sha").len(), 64);
    assert!(started["prompt"]
        .as_str()
        .expect("prompt")
        .contains("Idle animation"));
    assert_eq!(started["mode"], "gemini_api");
    assert_eq!(lines[2]["outcome"]["kind"], "video_inline");
    let downloaded = &lines[3];
    assert_eq!(
        downloaded["bytes"].as_u64().expect("bytes"),
        fixture_bytes().len() as u64
    );
    assert!(downloaded["frames"].as_u64().expect("frames") > 0);
    assert!(Path::new(downloaded["path"].as_str().expect("path")).is_file());

    // The key was used, and appears nowhere it could be read back.
    assert!(shim.stdin().contains(FAKE_KEY), "the run must authenticate");
    assert_no_key(
        &std::fs::read_to_string(&dirs.ledger).expect("ledger"),
        "the ledger",
    );
    assert_no_key(&String::from_utf8_lossy(&output.stdout), "stdout");
    assert_no_key(&String::from_utf8_lossy(&output.stderr), "stderr");
    assert_no_key(&dirs.clip_records(), "the clip records");
    assert!(
        !std::fs::read_to_string(&dirs.ledger)
            .expect("ledger")
            .to_lowercase()
            .contains("x-goog-api-key"),
        "no header may be recorded"
    );
}

#[test]
fn d3_the_key_never_leaks_from_a_failing_run_either() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = FakeServer::start(|request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(r#"{"name":"operations/doomed"}"#)
        } else {
            Reply::json(&format!(
                r#"{{"name":"operations/doomed","done":true,"error":{{"code":13,"message":"bad key {FAKE_KEY}"}}}}"#
            ))
        }
    });
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();

    let output = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);

    assert!(!output.status.success());
    assert_no_key(
        &std::fs::read_to_string(&dirs.ledger).expect("ledger"),
        "the ledger",
    );
    assert_no_key(&String::from_utf8_lossy(&output.stdout), "stdout");
    assert_no_key(&String::from_utf8_lossy(&output.stderr), "stderr");
    assert_no_key(&dirs.clip_records(), "the clip records");
}

// --- D-4: caps, rolling budget, quota stop, stop on the unexpected ---------------------------

#[test]
fn d4_max_generations_is_required_for_a_paid_run() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = silent_server();
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();

    let output = dirs.generate(&shim, &[&unit], &[]);

    assert!(!output.status.success(), "{}", text(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--max-generations"),
        "{}",
        text(&output)
    );
    assert!(server.requests().is_empty());
}

#[test]
fn d4_a_run_needing_more_than_its_cap_is_refused_before_any_request() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = silent_server();
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();

    let over = dirs.generate(&shim, &[&unit], &["--max-generations", "2"]);
    assert!(!over.status.success(), "{}", text(&over));
    assert!(
        String::from_utf8_lossy(&over.stderr).contains("needs 3"),
        "{}",
        text(&over)
    );

    let above_data_cap = dirs.generate(&shim, &[&unit], &["--max-generations", "13"]);
    assert!(
        !above_data_cap.status.success(),
        "{}",
        text(&above_data_cap)
    );
    assert!(
        String::from_utf8_lossy(&above_data_cap.stderr).contains("12"),
        "{}",
        text(&above_data_cap)
    );

    assert!(server.requests().is_empty());
    assert!(!dirs.ledger.exists());
}

/// A fixture `request_started` ledger line, `age_days` old.
fn started_line(age_days: u64, index: usize) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    json!({
        "time_utc": "2026-01-01T00:00:00Z",
        "unix_seconds": now - age_days * 86_400,
        "run_id": "fixture",
        "event": "request_started",
        "request_id": format!("fixture/{index}"),
        "slug": "fixture_identity",
        "state": "idle",
        "model": "fixture-model",
        "mode": "gemini_api",
        "prompt_sha256": "0".repeat(64),
        "prompt": "fixture prompt",
        "keyframe_sha256": null,
        "parameters": {}
    })
    .to_string()
}

#[test]
fn d4_the_rolling_seven_day_budget_refuses_an_over_budget_run() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = FakeServer::start(healthy(fixture_bytes()));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    std::fs::create_dir_all(dirs.ledger.parent().expect("parent")).expect("evidence dir");
    let recent: Vec<String> = (0..23).map(|i| started_line(1, i)).collect();
    std::fs::write(&dirs.ledger, recent.join("\n") + "\n").expect("ledger");

    let refused = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);
    assert!(!refused.status.success(), "{}", text(&refused));
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("23 of 25"),
        "{}",
        text(&refused)
    );
    assert!(server.requests().is_empty());

    // Lines older than the window do not count.
    let old: Vec<String> = (0..23).map(|i| started_line(8, i)).collect();
    std::fs::write(&dirs.ledger, old.join("\n") + "\n").expect("ledger");
    let allowed = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);
    assert!(allowed.status.success(), "{}", text(&allowed));
    assert_eq!(starts(&server), 3);
}

#[test]
fn d4_the_first_quota_refusal_ends_the_run_with_exit_code_3() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let healthy = healthy(fixture_bytes());
    let server = FakeServer::start(move |request, index| {
        let quota = r#"{"error":{"code":429,"message":"Quota exceeded for aiplatform.googleapis.com/generate_content_requests","status":"RESOURCE_EXHAUSTED"}}"#;
        if request.path.contains(":predictLongRunning") && state_of_body(&request.body) == "walk" {
            Reply::status(429, quota)
        } else {
            healthy(request, index)
        }
    });
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();

    let output = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);

    assert_eq!(output.status.code(), Some(3), "{}", text(&output));
    assert_eq!(starts(&server), 2, "the refused start is not retried");
    let events = dirs.ledger_events();
    assert_eq!(
        events,
        [
            "request_started",
            "operation_named",
            "operation_done",
            "downloaded",
            "request_started",
            "quota_refused"
        ]
    );
    let refusal = dirs.ledger_lines().pop().expect("last line");
    assert_eq!(refusal["http_status"], 429);
    assert_eq!(refusal["status"], "RESOURCE_EXHAUSTED");
}

/// A server whose operations finish with `done` as the polled body.
fn finishing_with(done: Value) -> FakeServer {
    FakeServer::start(move |request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(r#"{"name":"operations/unexpected"}"#)
        } else {
            Reply::json(&done.to_string())
        }
    })
}

/// The JSON operation records kept under the clips directory.
fn operation_records(clips: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![clips.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn d4_a_gcs_uri_result_stops_the_run_and_keeps_the_response() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = finishing_with(json!({
        "name": "operations/unexpected", "done": true,
        "response": { "videos": [{ "gcsUri": "gs://bucket/clip.mp4" }] }
    }));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();

    let output = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);

    assert_eq!(output.status.code(), Some(4), "{}", text(&output));
    assert_eq!(starts(&server), 1);
    let done = dirs.ledger_lines().pop().expect("line");
    assert_eq!(done["event"], "operation_done");
    assert_eq!(done["outcome"]["kind"], "gcs_uri");
    assert_eq!(done["outcome"]["uri"], "gs://bucket/clip.mp4");
    let records = operation_records(&dirs.clips);
    assert_eq!(records.len(), 1, "{records:?}");
    assert!(std::fs::read_to_string(&records[0])
        .expect("record")
        .contains("gs://bucket/clip.mp4"));
}

#[test]
fn d4_a_filtered_result_stops_the_run() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = finishing_with(json!({
        "name": "operations/unexpected", "done": true,
        "response": { "raiMediaFilteredCount": 1, "raiMediaFilteredReasons": ["violence"], "videos": [] }
    }));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();

    let output = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);

    assert_eq!(output.status.code(), Some(4), "{}", text(&output));
    assert_eq!(starts(&server), 1);
    let done = dirs.ledger_lines().pop().expect("line");
    assert_eq!(done["outcome"]["kind"], "filtered");
    assert!(done["outcome"]["reason"]
        .as_str()
        .expect("reason")
        .contains("violence"));
    assert_eq!(operation_records(&dirs.clips).len(), 1);
}

#[test]
fn d4_a_new_failure_kind_stops_the_run_and_a_known_one_does_not() {
    let catalog = shipped_catalog();
    let first = combat_unit(&catalog);
    let second = another_combat_unit(&catalog, &first);
    let server = finishing_with(json!({
        "name": "operations/unexpected", "done": true,
        "error": { "code": 13, "message": "internal error" }
    }));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    let args = ["--states", "idle", "--max-generations", "2"];

    let output = dirs.generate(&shim, &[&first, &second], &args);
    assert_eq!(output.status.code(), Some(4), "{}", text(&output));
    assert_eq!(starts(&server), 1, "an unseen failure kind stops the run");
    assert_eq!(operation_records(&dirs.clips).len(), 1);

    // Code 13 is now in the ledger, so the same failure no longer stops the run. The failed
    // clip is now a regeneration, which needs the explicit flag.
    let again = dirs.generate(
        &shim,
        &[&first, &second],
        &[
            "--states",
            "idle",
            "--max-generations",
            "2",
            "--allow-regeneration",
        ],
    );
    assert!(!again.status.success(), "{}", text(&again));
    assert_eq!(
        starts(&server),
        3,
        "a known failure kind moves on to the next clip"
    );
}

#[test]
fn d4_a_regeneration_needs_the_explicit_flag() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = finishing_with(json!({
        "name": "operations/unexpected", "done": true,
        "error": { "code": 13, "message": "internal error" }
    }));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    let _ = dirs.generate(
        &shim,
        &[&unit],
        &["--states", "idle", "--max-generations", "1"],
    );

    let dry = dirs.generate(&shim, &[&unit], &["--states", "idle", "--dry-run"]);
    assert!(
        stdout(&dry).contains(&format!("clip {unit} idle: regenerate")),
        "{}",
        text(&dry)
    );
    let refused = dirs.generate(
        &shim,
        &[&unit],
        &["--states", "idle", "--max-generations", "1"],
    );
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("--allow-regeneration"),
        "{}",
        text(&refused)
    );
    assert_eq!(starts(&server), 1);
}

#[test]
fn d4_an_interrupted_run_resumes_the_recorded_operation_instead_of_paying_again() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let outage = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let server_outage = std::sync::Arc::clone(&outage);
    let healthy = healthy(fixture_bytes());
    let server = FakeServer::start(move |request, index| {
        let down = server_outage.load(std::sync::atomic::Ordering::SeqCst);
        if down && request.path.contains("/operations/") {
            Reply::status(404, "operation lookup failed")
        } else {
            healthy(request, index)
        }
    });
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    let args = ["--states", "idle", "--max-generations", "1"];

    let first = dirs.generate(&shim, &[&unit], &args);
    assert!(!first.status.success(), "{}", text(&first));
    assert_eq!(starts(&server), 1);

    let dry = dirs.generate(&shim, &[&unit], &["--states", "idle", "--dry-run"]);
    assert!(
        stdout(&dry).contains(&format!("clip {unit} idle: resume operations/idle-op")),
        "{}",
        text(&dry)
    );
    assert!(
        stdout(&dry).contains("Clips to generate: 0"),
        "{}",
        text(&dry)
    );

    outage.store(false, std::sync::atomic::Ordering::SeqCst);
    let second = dirs.generate(&shim, &[&unit], &args);
    assert!(second.status.success(), "{}", text(&second));
    assert_eq!(
        starts(&server),
        1,
        "the paid operation is resumed, not restarted"
    );
    assert_eq!(
        dirs.ledger_events().last().map(String::as_str),
        Some("downloaded")
    );
}

#[test]
fn d4_a_corrupt_ledger_stops_the_run_before_any_request() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = silent_server();
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    std::fs::create_dir_all(dirs.ledger.parent().expect("parent")).expect("dir");
    std::fs::write(&dirs.ledger, "not json\n").expect("ledger");

    let output = dirs.generate(&shim, &[&unit], &["--max-generations", "3"]);

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("line 1"),
        "{}",
        text(&output)
    );
    assert!(server.requests().is_empty());
}

// --- Request parameters in data (letterbox plan `2026-11-04-veo-letterbox-bake.md`, D-6) ------

/// Copies the shipped catalogs and media data into a temporary bundle, with the request
/// settings replaced by `settings`.
fn bundle_with_request(settings: &str) -> tempfile::TempDir {
    let bundle = bundle_with_style(|style| style);
    std::fs::write(bundle.path().join("media/veo_request.ron"), settings).expect("settings");
    bundle
}

/// The `parameters` object of every start request the server received.
fn sent_parameters(server: &FakeServer) -> Vec<Value> {
    server
        .requests()
        .iter()
        .filter(|request| request.path.contains(":predictLongRunning"))
        .map(|request| {
            let body: Value = serde_json::from_slice(&request.body).expect("start body is JSON");
            body["parameters"].clone()
        })
        .collect()
}

/// The `parameters` of every `request_started` line of the run's ledger.
fn recorded_parameters(dirs: &RunDirs) -> Vec<Value> {
    dirs.ledger_lines()
        .iter()
        .filter(|line| line["event"] == "request_started")
        .map(|line| line["parameters"].clone())
        .collect()
}

#[test]
fn d6_the_shipped_settings_send_generate_audio_false_and_the_ledger_records_it() {
    let unit = combat_unit(&shipped_catalog());
    let (dirs, server, _shim, output) = generated(&unit, "idle");
    assert!(output.status.success(), "{}", text(&output));
    let sent = sent_parameters(&server);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["generateAudio"], json!(false), "{}", sent[0]);
    assert_eq!(sent[0]["aspectRatio"], "16:9");
    assert_eq!(recorded_parameters(&dirs), sent);
    assert!(
        stdout(&output).contains("generateAudio false"),
        "{}",
        text(&output)
    );
}

#[test]
fn d6_generate_audio_none_sends_no_such_key() {
    let unit = combat_unit(&shipped_catalog());
    let bundle = bundle_with_request(
        "(aspect_ratio: \"16:9\", duration_seconds: 4, sample_count: 1, generate_audio: None)",
    );
    let server = FakeServer::start(healthy(fixture_bytes()));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    let output = dirs.generate_in(
        bundle.path(),
        &shim,
        &[&unit],
        &["--states", "idle", "--max-generations", "1"],
    );
    assert!(output.status.success(), "{}", text(&output));
    let sent = sent_parameters(&server);
    assert_eq!(sent.len(), 1);
    assert!(sent[0].get("generateAudio").is_none(), "{}", sent[0]);
    assert_eq!(sent[0]["durationSeconds"], 4);
    assert_eq!(recorded_parameters(&dirs), sent);
}

#[test]
fn d6_malformed_request_settings_fail_before_any_credential_is_read() {
    let unit = combat_unit(&shipped_catalog());
    let cases = [
        "(aspect_ratio: \"16:9\", duration_seconds: 4)",
        "(aspect_ratio: \"16:9\", duration_seconds: 4, sample_count: 2, generate_audio: None)",
        "(aspect_ratio: \"16:9\", duration_seconds: 4, sample_count: 1, generate_audio: None, extra: 1)",
    ];
    for settings in cases {
        let bundle = bundle_with_request(settings);
        let server = silent_server();
        let shim = CurlShim::new(&server);
        let dirs = RunDirs::new();
        // Trap the credentials: a `.env` that is a directory cannot be read, so a run that
        // read credentials first would fail with that I/O error instead.
        std::fs::remove_file(dirs.credentials.join(".env")).expect("remove key file");
        std::fs::create_dir_all(dirs.credentials.join(".env")).expect("trap .env");
        let output = dirs.generate_in(
            bundle.path(),
            &shim,
            &[&unit],
            &["--states", "idle", "--max-generations", "1"],
        );
        assert!(!output.status.success(), "{settings}: {}", text(&output));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("invalid data file") && stderr.contains("veo_request.ron"),
            "{settings}: {}",
            text(&output)
        );
        assert!(server.requests().is_empty());
        assert!(!dirs.ledger.exists(), "nothing may be recorded");
    }
}

#[test]
fn d6_the_committed_ledger_still_parses_and_its_pilot_sent_no_audio_field() {
    let ledger = shipped_assets().join("../docs/evidence/veo/ledger.jsonl");
    let records = asset_baker::ledger::read_ledger_numbered(&ledger).expect("committed ledger");
    let started: Vec<&asset_baker::VeoParameters> = records
        .iter()
        .filter_map(|numbered| match &numbered.record.event {
            asset_baker::ledger::LedgerEvent::RequestStarted { parameters, .. } => Some(parameters),
            _ => None,
        })
        .collect();
    assert!(!started.is_empty(), "the pilot's request is recorded");
    assert!(started.iter().all(|p| p.generate_audio.is_none()));
}

// --- D-5: free preflight ---------------------------------------------------------------------

fn preflight(shim: &CurlShim, credentials: &Path) -> std::process::Output {
    baker()
        .args(["veo-preflight", "--model", "veo-fixture-model"])
        .arg("--credentials-dir")
        .arg(credentials)
        .arg("--curl-path")
        .arg(shim.path())
        .output()
        .expect("run baker")
}

#[test]
fn d5_preflight_makes_one_authenticated_get_and_prints_only_the_status() {
    let server = FakeServer::start(|_, _| {
        Reply::json(r#"{"name":"models/veo-fixture-model","displayName":"Fixture"}"#)
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_credentials(dir.path());

    let output = preflight(&shim, &credentials);

    assert!(output.status.success(), "{}", text(&output));
    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "GET");
    assert_eq!(
        requests[0].path,
        "/generativelanguage.googleapis.com/v1beta/models/veo-fixture-model"
    );
    assert!(requests[0].header_contains(&format!("x-goog-api-key: {FAKE_KEY}")));
    let out = stdout(&output);
    assert!(out.contains("HTTP status: 200"), "{out}");
    assert!(out.contains("Model visible: yes"), "{out}");
    assert_no_key(&out, "stdout");
    assert_no_key(&String::from_utf8_lossy(&output.stderr), "stderr");
}

#[test]
fn d5_a_refused_preflight_fails_and_still_prints_no_key() {
    let server = FakeServer::start(|_, _| {
        Reply::status(
            403,
            &format!(
                r#"{{"error":{{"code":403,"message":"key {FAKE_KEY} denied","status":"PERMISSION_DENIED"}}}}"#
            ),
        )
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_credentials(dir.path());

    let output = preflight(&shim, &credentials);

    assert!(!output.status.success(), "{}", text(&output));
    let out = stdout(&output);
    assert!(out.contains("HTTP status: 403"), "{out}");
    assert!(out.contains("PERMISSION_DENIED"), "{out}");
    assert!(out.contains("Model visible: no"), "{out}");
    assert_no_key(&out, "stdout");
    assert_no_key(&String::from_utf8_lossy(&output.stderr), "stderr");
    assert_eq!(server.requests().len(), 1);
}

// --- D-7: keyframe chaining, keyframe extraction, review, install, import --------------------

/// Mean luma of the image a start request carried, if any.
fn request_image_luma(body: &[u8]) -> Option<f64> {
    let json: Value = serde_json::from_slice(body).expect("JSON body");
    let encoded = json["instances"][0]["image"]["bytesBase64Encoded"].as_str()?;
    let bytes = BASE64_STANDARD.decode(encoded).expect("base64 image");
    let image = image::load_from_memory(&bytes)
        .expect("decodable image")
        .to_luma8();
    let sum: u64 = image.pixels().map(|p| u64::from(p.0[0])).sum();
    Some(sum as f64 / f64::from(image.width() * image.height()))
}

#[test]
fn d7_idle_is_generated_first_and_its_frame_at_one_second_seeds_the_other_states() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let (_dirs, server, _shim, output) = generated(&unit, "attack,walk,idle");
    assert!(output.status.success(), "{}", text(&output));

    let started: Vec<_> = server
        .requests()
        .into_iter()
        .filter(|request| request.path.contains(":predictLongRunning"))
        .collect();
    let order: Vec<&str> = started.iter().map(|r| state_of_body(&r.body)).collect();
    assert_eq!(order, ["idle", "walk", "attack"]);
    assert_eq!(
        request_image_luma(&started[0].body),
        None,
        "idle is text-to-video"
    );
    for request in &started[1..] {
        // The fixture's luma rises from 16 to 235 over 4 s, about 64 in full range at 1.0 s.
        let luma = request_image_luma(&request.body).expect("chained keyframe");
        assert!(
            (48.0..=80.0).contains(&luma),
            "frame is not from 1.0 s: {luma}"
        );
    }
}

#[test]
fn d7_keyframe_from_clip_extracts_the_frame_at_the_requested_time() {
    let dir = tempfile::tempdir().expect("tempdir");
    let early = dir.path().join("early.png");
    let late = dir.path().join("late.png");
    for (at, out) in [("1.0", &early), ("3.0", &late)] {
        let output = baker()
            .arg("keyframe-from-clip")
            .arg("--clip")
            .arg(common::fixture_clip())
            .args(["--at", at])
            .arg("--output")
            .arg(out)
            .output()
            .expect("run baker");
        assert!(output.status.success(), "{}", text(&output));
    }
    let mean = |path: &Path| {
        let image = image::open(path).expect("png").to_luma8();
        let sum: u64 = image.pixels().map(|p| u64::from(p.0[0])).sum();
        sum as f64 / f64::from(image.width() * image.height())
    };
    assert!((48.0..=80.0).contains(&mean(&early)), "{}", mean(&early));
    assert!(mean(&late) > mean(&early) + 100.0, "{}", mean(&late));
}

/// The path of the clip a run downloaded for `state`.
fn downloaded_clip(dirs: &RunDirs, state: &str) -> PathBuf {
    dirs.ledger_lines()
        .iter()
        .find(|line| line["event"] == "downloaded" && line["state"] == state)
        .map(|line| PathBuf::from(line["path"].as_str().expect("path")))
        .expect("a downloaded clip")
}

fn install(dirs: &RunDirs, unit: &str, state: &str, clip: &Path) -> std::process::Output {
    baker()
        .args(["install-clip", "--unit", unit, "--state", state])
        .arg("--clip")
        .arg(clip)
        .arg("--ledger")
        .arg(&dirs.ledger)
        .arg("--videos-dir")
        .arg(&dirs.videos)
        .output()
        .expect("run baker")
}

#[test]
fn d7_install_clip_accepts_only_a_validated_clip_the_ledger_records() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let (dirs, _server, _shim, output) = generated(&unit, "idle");
    assert!(output.status.success(), "{}", text(&output));
    let clip = downloaded_clip(&dirs, "idle");

    let wrong_state = install(&dirs, &unit, "walk", &clip);
    assert!(!wrong_state.status.success(), "{}", text(&wrong_state));
    assert!(
        String::from_utf8_lossy(&wrong_state.stderr).contains("ledger"),
        "{}",
        text(&wrong_state)
    );

    let garbage = dirs.root.path().join("garbage.mp4");
    std::fs::write(&garbage, b"not a video").expect("garbage");
    let invalid = install(&dirs, &unit, "idle", &garbage);
    assert!(!invalid.status.success(), "{}", text(&invalid));

    let installed = install(&dirs, &unit, "idle", &clip);
    assert!(installed.status.success(), "{}", text(&installed));
    let target = dirs.videos.join(format!("{unit}_idle.mp4"));
    assert_eq!(std::fs::read(&target).expect("installed"), fixture_bytes());

    let again = install(&dirs, &unit, "idle", &clip);
    assert!(
        !again.status.success(),
        "an installed clip is not replaced silently"
    );
}

#[test]
fn d7_import_needs_an_operation_the_ledger_names() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let server = finishing_with(json!({
        "name": "operations/unexpected", "done": true,
        "response": { "videos": [{ "gcsUri": "gs://bucket/clip.mp4" }] }
    }));
    let shim = CurlShim::new(&server);
    let dirs = RunDirs::new();
    let stopped = dirs.generate(
        &shim,
        &[&unit],
        &["--states", "idle", "--max-generations", "1"],
    );
    assert_eq!(stopped.status.code(), Some(4), "{}", text(&stopped));
    let clip = common::fixture_clip();
    let clip = clip.to_str().expect("utf-8 path");

    let without = dirs.generate(&shim, &[], &["--import", &unit, "idle", clip]);
    assert!(!without.status.success(), "{}", text(&without));
    assert!(
        String::from_utf8_lossy(&without.stderr).contains("--operation"),
        "{}",
        text(&without)
    );

    let unknown = dirs.generate(
        &shim,
        &[],
        &[
            "--import",
            &unit,
            "idle",
            clip,
            "--operation",
            "operations/other",
        ],
    );
    assert!(!unknown.status.success(), "{}", text(&unknown));

    let imported = dirs.generate(
        &shim,
        &[],
        &[
            "--import",
            &unit,
            "idle",
            clip,
            "--operation",
            "operations/unexpected",
        ],
    );
    assert!(imported.status.success(), "{}", text(&imported));
    let last = dirs.ledger_lines().pop().expect("line");
    assert_eq!(last["event"], "downloaded");
    assert_eq!(last["operation"], "operations/unexpected");
    assert!(Path::new(last["path"].as_str().expect("path")).is_file());
    assert_eq!(starts(&server), 1, "importing spends nothing");
}

#[test]
fn d7_review_writes_contact_sheets_and_records_a_rejection() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let (dirs, _server, shim, output) = generated(&unit, "idle");
    assert!(output.status.success(), "{}", text(&output));
    let sheets = dirs.root.path().join("sheets");
    let evidence = dirs.root.path().join("evidence-review");
    let review = |extra: &[&str]| {
        baker()
            .args(["review", "--unit", &unit])
            .arg("--videos-dir")
            .arg(&dirs.videos)
            .arg("--ledger")
            .arg(&dirs.ledger)
            .arg("--sheets-dir")
            .arg(&sheets)
            .arg("--evidence-dir")
            .arg(&evidence)
            .args(extra)
            .output()
            .expect("run baker")
    };

    let made = review(&[]);
    assert!(made.status.success(), "{}", text(&made));
    let full = sheets.join(format!("{unit}_idle.png"));
    let small = evidence.join(format!("{unit}_idle.png"));
    let (full_w, _) = image::image_dimensions(&full).expect("full sheet");
    let (small_w, _) = image::image_dimensions(&small).expect("evidence sheet");
    assert!(full_w >= 8 * 16, "eight frames side by side: {full_w}");
    assert!(small_w <= 1024);

    let rejected = review(&[
        "--reject",
        "idle",
        "--reason",
        "the subject leaves the crop",
    ]);
    assert!(rejected.status.success(), "{}", text(&rejected));
    let last = dirs.ledger_lines().pop().expect("line");
    assert_eq!(last["event"], "rejected_in_review");
    assert_eq!(last["reason"], "the subject leaves the crop");
    assert_eq!(last["sha256"].as_str().expect("sha").len(), 64);

    let dry = dirs.generate(&shim, &[&unit], &["--states", "idle", "--dry-run"]);
    assert!(
        stdout(&dry).contains(&format!("clip {unit} idle: regenerate")),
        "{}",
        text(&dry)
    );
    let clip = downloaded_clip(&dirs, "idle");
    let install_rejected = install(&dirs, &unit, "idle", &clip);
    assert!(
        !install_rejected.status.success(),
        "a rejected clip is never installed"
    );
}

#[test]
fn d7_a_clip_awaiting_review_is_not_paid_for_again() {
    let catalog = shipped_catalog();
    let unit = combat_unit(&catalog);
    let (dirs, server, shim, output) = generated(&unit, "idle");
    assert!(output.status.success(), "{}", text(&output));

    let dry = dirs.generate(&shim, &[&unit], &["--dry-run"]);
    let out = stdout(&dry);
    assert!(
        out.contains(&format!("clip {unit} idle: awaiting review")),
        "{out}"
    );
    assert!(out.contains("Clips to generate: 2"), "{out}");

    let rest = dirs.generate(&shim, &[&unit], &["--max-generations", "2"]);
    assert!(rest.status.success(), "{}", text(&rest));
    assert_eq!(starts(&server), 3, "idle is not generated twice");
    let _ = inline_done;
}
