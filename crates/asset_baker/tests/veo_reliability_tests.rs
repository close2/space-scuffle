//! Behavioural tests of Veo download integrity and of polling resilience.
//!
//! A failed download must never enter the video cache, and a transient fault must never lose
//! an operation that has already been paid for. As in `veo_transport_tests.rs`, all traffic
//! goes through the real curl to a loopback server, and the key is fake.

#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::veo_client::{VeoClient, VeoCredentials, VeoOperation};
use asset_baker::{BakerError, UnitMediaDirs};
use base64::prelude::*;
use common::{CurlShim, FakeServer, Reply};
use serde_json::json;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod common;

/// The shipped unit catalog; the baker itself loads it from disk at run time.
fn catalog() -> sim_core::UnitCatalog {
    sim_fixtures::shipped_catalogs().units
}

const KEY: &str = "AIzaFAKE-test-only-not-a-credential";
const DOWNLOAD_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/files/abc:download?alt=media";
const HTML_ERROR: &str = "<html><body>Error 403 Forbidden, this is not a video</body></html>";
const CATALOG_UNIT: &str = "pulse_bot";

fn client(shim: &CurlShim) -> VeoClient {
    VeoClient::with_credentials(VeoCredentials {
        api_key: KEY.to_string(),
        project_id: Some("test-project".to_string()),
    })
    .with_curl_path(shim.path())
    .with_poll_interval(Duration::from_millis(1))
    .with_retry_policy(3, Duration::from_millis(1))
}

fn uri_operation() -> VeoOperation {
    VeoOperation {
        name: "operations/finished".to_string(),
        done: true,
        response: Some(json!({
            "generateVideoResponse": { "generatedSamples": [{ "video": { "uri": DOWNLOAD_URL } }] }
        })),
        error: None,
    }
}

fn inline_operation(bytes: &[u8]) -> VeoOperation {
    VeoOperation {
        name: "operations/inline".to_string(),
        done: true,
        response: Some(
            json!({ "videos": [{ "bytesBase64Encoded": BASE64_STANDARD.encode(bytes) }] }),
        ),
        error: None,
    }
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(common::fixture_clip()).expect("read fixture clip")
}

fn directory_entries(dir: &Path) -> Vec<String> {
    let mut entries: Vec<String> = std::fs::read_dir(dir)
        .expect("read directory")
        .map(|entry| {
            entry
                .expect("directory entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    entries.sort();
    entries
}

fn assert_nothing_cached(dir: &Path) {
    assert_eq!(
        directory_entries(dir),
        Vec::<String>::new(),
        "a failed download left files behind"
    );
}

#[test]
fn b3_http_error_body_never_becomes_a_video() {
    let server = FakeServer::start(|_, _| Reply::status(403, HTML_ERROR));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    let result = client(&shim).save_operation_video(&uri_operation(), &dir.path().join("a.mp4"));

    match result {
        Err(BakerError::VeoHttpStatus { status, body }) => {
            assert_eq!(status, 403);
            assert!(body.contains("Forbidden"), "body should explain: {body}");
        }
        other => panic!("expected VeoHttpStatus, got {other:?}"),
    }
    assert_nothing_cached(dir.path());
}

#[test]
fn b3_success_status_with_a_non_video_body_is_rejected() {
    let server = FakeServer::start(|_, _| Reply::bytes(HTML_ERROR.as_bytes().to_vec()));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    let result = client(&shim).save_operation_video(&uri_operation(), &dir.path().join("a.mp4"));

    assert!(
        matches!(result, Err(BakerError::VideoValidationFailed { .. })),
        "{result:?}"
    );
    assert_nothing_cached(dir.path());
}

#[test]
fn b3_truncated_video_is_rejected() {
    let mut bytes = fixture_bytes();
    bytes.truncate(bytes.len() * 2 / 3);
    let server = FakeServer::start(move |_, _| Reply::bytes(bytes.clone()));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    let result = client(&shim).save_operation_video(&uri_operation(), &dir.path().join("a.mp4"));

    assert!(
        matches!(result, Err(BakerError::VideoValidationFailed { .. })),
        "{result:?}"
    );
    assert_nothing_cached(dir.path());
}

#[test]
fn b3_inline_bytes_are_validated_too() {
    let server = FakeServer::start(|_, _| Reply::bytes(Vec::new()));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    let result = client(&shim).save_operation_video(
        &inline_operation(HTML_ERROR.as_bytes()),
        &dir.path().join("a.mp4"),
    );

    assert!(
        matches!(result, Err(BakerError::VideoValidationFailed { .. })),
        "{result:?}"
    );
    assert_nothing_cached(dir.path());
}

#[test]
fn b3_failed_download_preserves_the_existing_file() {
    let server = FakeServer::start(|_, _| Reply::status(500, HTML_ERROR));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let destination = dir.path().join("a.mp4");
    std::fs::write(&destination, b"previous").expect("seed file");

    let result = client(&shim).save_operation_video(&uri_operation(), &destination);

    assert!(result.is_err());
    assert_eq!(std::fs::read(&destination).expect("file"), b"previous");
}

#[test]
fn b3_valid_video_is_installed_at_the_destination() {
    let bytes = fixture_bytes();
    let expected = bytes.clone();
    let server = FakeServer::start(move |_, _| Reply::bytes(bytes.clone()));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let destination = dir.path().join("a.mp4");

    client(&shim)
        .save_operation_video(&uri_operation(), &destination)
        .expect("valid video");

    assert_eq!(std::fs::read(&destination).expect("file"), expected);
    assert_eq!(directory_entries(dir.path()), vec!["a.mp4".to_string()]);
}

#[test]
fn b3_poisoned_cache_entry_is_refused_without_spending_money() {
    let server = FakeServer::start(|_, _| Reply::status(500, "must not be called"));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let videos = dir.path().join("videos");
    std::fs::create_dir_all(&videos).expect("videos dir");
    let poisoned = videos.join(format!("{CATALOG_UNIT}_idle.mp4"));
    // Larger than the old 1000-byte heuristic, which accepted any such file.
    std::fs::write(&poisoned, HTML_ERROR.repeat(50)).expect("poison cache");

    let result = client(&shim).generate_and_bake_unit(
        &catalog(),
        CATALOG_UNIT,
        &UnitMediaDirs {
            keyframes: &dir.path().join("keyframes"),
            videos: &videos,
            output: &dir.path().join("out"),
        },
        64,
        5,
    );

    match result {
        Err(BakerError::CachedVideoInvalid { path, .. }) => assert_eq!(path, poisoned),
        other => panic!("expected CachedVideoInvalid, got {other:?}"),
    }
    assert!(server.requests().is_empty(), "no API call may be made");
}

#[test]
fn b4_polling_survives_transient_faults() {
    let server = FakeServer::start(|_, index| match index {
        0 => Reply::status(503, "unavailable"),
        1 => Reply::json("<html>a proxy said hello</html>"),
        2 => Reply::status(429, "slow down"),
        _ => Reply::json(r#"{"name":"operations/op1","done":true}"#),
    });
    let shim = CurlShim::new(&server);

    let operation = client(&shim)
        .poll_operation("operations/op1")
        .expect("transient faults must not lose the operation");

    assert!(operation.done);
    assert_eq!(server.requests().len(), 4);
}

#[test]
fn b4_a_successful_poll_resets_the_transient_budget() {
    let server = FakeServer::start(|_, index| match index % 3 {
        0 | 1 if index < 12 => Reply::status(503, "unavailable"),
        2 if index < 12 => Reply::json(r#"{"name":"operations/op1","done":false}"#),
        _ => Reply::json(r#"{"name":"operations/op1","done":true}"#),
    });
    let shim = CurlShim::new(&server);

    client(&shim)
        .with_retry_policy(2, Duration::from_millis(1))
        .poll_operation("operations/op1")
        .expect("only consecutive faults count");
}

#[test]
fn b4_polling_gives_up_after_bounded_retries_and_names_the_operation() {
    let server = FakeServer::start(|_, _| Reply::status(503, "unavailable"));
    let shim = CurlShim::new(&server);

    let result = client(&shim).poll_operation("operations/op1");

    match result {
        Err(BakerError::VeoTransientExhausted {
            operation,
            attempts,
            ..
        }) => {
            assert_eq!(operation, "operations/op1");
            assert_eq!(attempts, 4);
        }
        other => panic!("expected VeoTransientExhausted, got {other:?}"),
    }
    assert_eq!(server.requests().len(), 4, "1 attempt plus 3 retries");
}

#[test]
fn b4_permanent_http_errors_are_not_retried() {
    let server = FakeServer::start(|_, _| Reply::status(403, "forbidden"));
    let shim = CurlShim::new(&server);

    let result = client(&shim).poll_operation("operations/op1");

    assert!(
        matches!(result, Err(BakerError::VeoHttpStatus { status: 403, .. })),
        "{result:?}"
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn b4_a_server_that_never_answers_cannot_hang_polling() {
    let server = FakeServer::start(|_, _| Reply::Hang);
    let shim = CurlShim::new(&server);
    let started = Instant::now();

    let result = client(&shim)
        .with_poll_timeout(Duration::from_millis(1500))
        .with_retry_policy(1000, Duration::from_millis(1))
        .poll_operation("operations/op1");

    assert!(
        matches!(result, Err(BakerError::VeoTimeout { .. })),
        "{result:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "polling overran its deadline: {:?}",
        started.elapsed()
    );
}

#[test]
fn b4_initiate_retries_only_when_the_request_cannot_have_started_work() {
    let server = FakeServer::start(|_, index| {
        if index == 0 {
            Reply::status(429, "rate limited")
        } else {
            Reply::json(r#"{"name":"operations/op1"}"#)
        }
    });
    let shim = CurlShim::new(&server);
    let operation = client(&shim)
        .initiate_generation("prompt", None)
        .expect("429 means no work was started");
    assert_eq!(operation.name, "operations/op1");
    assert_eq!(server.requests().len(), 2);
    drop(shim);

    // A 500 may have started a paid generation, so a retry could pay twice.
    let server = FakeServer::start(|_, _| Reply::status(500, "ambiguous"));
    let shim = CurlShim::new(&server);
    let result = client(&shim).initiate_generation("prompt", None);
    assert!(
        matches!(result, Err(BakerError::VeoHttpStatus { status: 500, .. })),
        "{result:?}"
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn b4_download_retries_transient_faults() {
    let bytes = fixture_bytes();
    let server = FakeServer::start(move |_, index| {
        if index < 2 {
            Reply::status(503, "unavailable")
        } else {
            Reply::bytes(bytes.clone())
        }
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    client(&shim)
        .save_operation_video(&uri_operation(), &dir.path().join("a.mp4"))
        .expect("download must recover");

    assert_eq!(server.requests().len(), 3);
}

/// A healthy API: each initiate creates one operation, which is finished on first poll.
fn healthy_reply(request: &common::ReceivedRequest, bytes: &[u8]) -> Reply {
    if request.path.contains(":predictLongRunning") {
        let state = String::from_utf8_lossy(&request.body).to_lowercase();
        let name = ["idle", "walk", "attack"]
            .into_iter()
            .find(|state_name| state.contains(&format!("{state_name} animation")))
            .unwrap_or("unknown");
        Reply::json(&format!(r#"{{"name":"operations/{name}"}}"#))
    } else if request.path.contains("/operations/") {
        Reply::json(
            &json!({
                "name": "operations/x",
                "done": true,
                "response": { "generateVideoResponse": { "generatedSamples": [
                    { "video": { "uri": DOWNLOAD_URL } }
                ] } }
            })
            .to_string(),
        )
    } else {
        Reply::bytes(bytes.to_vec())
    }
}

fn count_initiates(server: &FakeServer) -> usize {
    server
        .requests()
        .iter()
        .filter(|request| request.path.contains(":predictLongRunning"))
        .count()
}

#[test]
fn b4_an_interrupted_run_resumes_the_paid_operation_instead_of_paying_again() {
    let bytes = fixture_bytes();
    let outage = Arc::new(AtomicBool::new(true));
    let server_outage = Arc::clone(&outage);
    let server = FakeServer::start(move |request, _| {
        if server_outage.load(Ordering::SeqCst) && request.path.contains("/operations/") {
            Reply::status(503, "unavailable")
        } else {
            healthy_reply(request, &bytes)
        }
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let videos = dir.path().join("videos");
    let run = |client: &VeoClient| {
        client.generate_and_bake_unit(
            &catalog(),
            CATALOG_UNIT,
            &UnitMediaDirs {
                keyframes: &dir.path().join("keyframes"),
                videos: &videos,
                output: &dir.path().join("out"),
            },
            64,
            5,
        )
    };

    let first = run(&client(&shim));
    assert!(
        matches!(first, Err(BakerError::VeoTransientExhausted { .. })),
        "{first:?}"
    );
    assert_eq!(count_initiates(&server), 1);
    let record = videos.join(format!("{CATALOG_UNIT}_idle.operation.json"));
    assert!(record.is_file(), "operation name must be persisted");

    outage.store(false, Ordering::SeqCst);
    let meta = run(&client(&shim)).expect("second run completes");

    assert_eq!(meta.name, CATALOG_UNIT);
    assert_eq!(
        count_initiates(&server),
        3,
        "idle must resume; only walk and attack are new"
    );
    assert!(
        !directory_entries(&videos)
            .iter()
            .any(|name| name.ends_with(".operation.json")),
        "finished operations must not stay on record"
    );
}

#[test]
fn b4_a_failed_download_keeps_the_record_so_the_result_is_not_paid_for_twice() {
    let bytes = fixture_bytes();
    let outage = Arc::new(AtomicBool::new(true));
    let server_outage = Arc::clone(&outage);
    let server = FakeServer::start(move |request, _| {
        if server_outage.load(Ordering::SeqCst) && request.path.contains(":download") {
            Reply::status(404, "gone for now")
        } else {
            healthy_reply(request, &bytes)
        }
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let videos = dir.path().join("videos");
    let run = |client: &VeoClient| {
        client.generate_and_bake_unit(
            &catalog(),
            CATALOG_UNIT,
            &UnitMediaDirs {
                keyframes: &dir.path().join("keyframes"),
                videos: &videos,
                output: &dir.path().join("out"),
            },
            64,
            5,
        )
    };

    assert!(run(&client(&shim)).is_err());
    assert_eq!(count_initiates(&server), 1);

    outage.store(false, Ordering::SeqCst);
    run(&client(&shim)).expect("second run completes");
    assert_eq!(count_initiates(&server), 3);
}

#[test]
fn b4_a_terminally_failed_operation_clears_its_record() {
    let server = FakeServer::start(|request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(r#"{"name":"operations/doomed"}"#)
        } else {
            Reply::json(
                r#"{"name":"operations/doomed","done":true,"error":{"message":"model refused"}}"#,
            )
        }
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let videos = dir.path().join("videos");

    let result = client(&shim).generate_and_bake_unit(
        &catalog(),
        CATALOG_UNIT,
        &UnitMediaDirs {
            keyframes: &dir.path().join("keyframes"),
            videos: &videos,
            output: &dir.path().join("out"),
        },
        64,
        5,
    );

    match result {
        Err(BakerError::VeoOperationFailed { operation, message }) => {
            assert_eq!(operation, "operations/doomed");
            assert!(message.contains("model refused"));
        }
        other => panic!("expected VeoOperationFailed, got {other:?}"),
    }
    assert_eq!(directory_entries(&videos), Vec::<String>::new());
}

#[test]
fn b4_an_unreadable_record_stops_the_run_instead_of_paying_again() {
    let server = FakeServer::start(|_, _| Reply::status(500, "must not be called"));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let videos = dir.path().join("videos");
    std::fs::create_dir_all(&videos).expect("videos dir");
    std::fs::write(
        videos.join(format!("{CATALOG_UNIT}_idle.operation.json")),
        "not json",
    )
    .expect("corrupt record");

    let result = client(&shim).generate_and_bake_unit(
        &catalog(),
        CATALOG_UNIT,
        &UnitMediaDirs {
            keyframes: &dir.path().join("keyframes"),
            videos: &videos,
            output: &dir.path().join("out"),
        },
        64,
        5,
    );

    assert!(result.is_err());
    assert!(server.requests().is_empty(), "no API call may be made");
}

#[test]
fn b9_only_exact_catalog_slugs_can_spend_money_or_touch_the_filesystem() {
    let server = FakeServer::start(|_, _| Reply::status(500, "must not be called"));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let videos = dir.path().join("videos");

    for unit in [
        "fictional_unit",
        "Pulse-Bot",
        "PULSE_BOT",
        "pulse bot",
        "Pulse Bot",
        "",
        "../../escaped",
        "pulse_bot/../../escaped",
        "/etc/escaped",
    ] {
        let result = client(&shim).generate_and_bake_unit(
            &catalog(),
            unit,
            &UnitMediaDirs {
                keyframes: &dir.path().join("keyframes"),
                videos: &videos,
                output: &dir.path().join("out"),
            },
            64,
            5,
        );

        assert!(
            matches!(result, Err(BakerError::UnknownArchetype { .. })),
            "{unit:?}: {result:?}"
        );
    }

    assert!(server.requests().is_empty(), "no API call may be made");
    assert!(!videos.exists(), "no directory may be created");
}
