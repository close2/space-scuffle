//! Behavioural tests of the Veo HTTP transport through the real curl executable.
//!
//! Every request goes through [`common::CurlShim`] to a loopback [`common::FakeServer`], so
//! no test can reach Google, and only obviously fake keys are used.

#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::veo_client::{VeoClient, VeoCredentials, VeoOperation};
use asset_baker::BakerError;
use common::{CurlShim, FakeServer, Reply};
use serde_json::json;
use std::path::Path;

mod common;

const GEMINI_KEY: &str = "AIzaFAKE-test-only-not-a-credential";
const VERTEX_KEY: &str = "AQ.FAKE-test-only-not-a-credential";
const DOWNLOAD_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/files/abc:download?alt=media";

fn client(key: &str, shim: &CurlShim) -> VeoClient {
    VeoClient::with_credentials(VeoCredentials {
        api_key: key.to_string(),
        project_id: Some("test-project".to_string()),
    })
    .with_curl_path(shim.path())
}

fn finished_operation(uri: &str) -> VeoOperation {
    VeoOperation {
        name: "operations/finished".to_string(),
        done: true,
        response: Some(json!({
            "generateVideoResponse": { "generatedSamples": [{ "video": { "uri": uri } }] }
        })),
        error: None,
    }
}

fn fixture_bytes() -> Vec<u8> {
    std::fs::read(common::fixture_clip()).expect("read fixture clip")
}

fn assert_key_only_on_stdin(shim: &CurlShim, key: &str) {
    assert!(
        !shim.argv().contains(key),
        "API key leaked into curl argv:\n{}",
        shim.argv()
    );
    assert!(
        shim.stdin().contains(key),
        "API key was expected in the curl config on stdin"
    );
}

/// The harness must contain a regression in which production passes `-L` again, because a
/// redirect in these tests can name a real trusted host.
#[test]
fn harness_blocks_a_redirect_even_if_curl_is_told_to_follow_it() {
    let server = FakeServer::start(|_, _| {
        Reply::redirect("https://us-central1-aiplatform.googleapis.com/must-not-be-reached")
    });
    let shim = CurlShim::new(&server);

    let output = std::process::Command::new(shim.path())
        .args(["-q", "--silent", "--location", "--output", "/dev/null"])
        .arg("https://generativelanguage.googleapis.com/start")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("run shim");

    assert_eq!(
        output.status.code(),
        Some(47),
        "curl must stop at the limit"
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn b2_key_is_never_in_argv_for_any_gemini_request() {
    let bytes = fixture_bytes();
    let server = FakeServer::start(move |request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(r#"{"name":"operations/op1"}"#)
        } else if request.path.contains("operations/op1") {
            Reply::json(r#"{"name":"operations/op1","done":true}"#)
        } else {
            Reply::bytes(bytes.clone())
        }
    });
    let shim = CurlShim::new(&server);
    let client = client(GEMINI_KEY, &shim);

    let operation = client
        .initiate_generation("prompt", None)
        .expect("initiate");
    client.poll_operation(&operation.name).expect("poll");
    let dir = tempfile::tempdir().expect("tempdir");
    client
        .save_operation_video(
            &finished_operation(DOWNLOAD_URL),
            &dir.path().join("out.mp4"),
        )
        .expect("download");

    assert_key_only_on_stdin(&shim, GEMINI_KEY);
    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        assert!(
            !request.path.contains(GEMINI_KEY),
            "API key leaked into a URL: {}",
            request.path
        );
        assert!(
            request.header_contains(&format!("x-goog-api-key: {GEMINI_KEY}")),
            "{} did not authenticate by header",
            request.path
        );
    }
}

#[test]
fn b2_key_is_never_in_argv_for_any_vertex_request() {
    let server = FakeServer::start(|request, _| {
        if request.path.contains(":predictLongRunning") {
            Reply::json(r#"{"name":"operations/op1"}"#)
        } else {
            Reply::json(r#"{"name":"operations/op1","done":true}"#)
        }
    });
    let shim = CurlShim::new(&server);
    let client = client(VERTEX_KEY, &shim);

    let operation = client
        .initiate_generation("prompt", None)
        .expect("initiate");
    client.poll_operation(&operation.name).expect("poll");

    assert_key_only_on_stdin(&shim, VERTEX_KEY);
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[1].method, "POST");
    assert!(String::from_utf8_lossy(&requests[1].body).contains("operations/op1"));
    for request in &requests {
        assert!(!request.path.contains(VERTEX_KEY));
        assert!(request.header_contains(&format!("x-goog-api-key: {VERTEX_KEY}")));
    }
}

#[test]
fn b2_request_body_reaches_the_server_unchanged() {
    let server = FakeServer::start(|_, _| Reply::json(r#"{"name":"operations/op1"}"#));
    let shim = CurlShim::new(&server);

    client(GEMINI_KEY, &shim)
        .initiate_generation("a prompt with \"quotes\"", None)
        .expect("initiate");

    let requests = server.requests();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
    assert_eq!(body["instances"][0]["prompt"], "a prompt with \"quotes\"");
}

#[test]
fn b2_curlrc_cannot_reenable_redirects() {
    let server = FakeServer::start(|_, _| Reply::json(r#"{"name":"operations/op1"}"#));
    let shim = CurlShim::new(&server);

    client(GEMINI_KEY, &shim)
        .initiate_generation("prompt", None)
        .expect("initiate");

    assert_eq!(
        shim.argv().lines().next(),
        Some("-q"),
        "-q must be the first argument so that a user .curlrc is ignored"
    );
    assert!(!shim.argv().lines().any(|arg| arg == "-L"));
}

#[test]
fn b2_redirects_are_followed_without_the_key() {
    let bytes = fixture_bytes();
    let server = FakeServer::start(move |request, _| {
        if request
            .path
            .starts_with("/generativelanguage.googleapis.com/")
        {
            Reply::redirect("https://first-hop.example.test/one")
        } else if request.path.starts_with("/first-hop.example.test/") {
            Reply::redirect("https://second-hop.example.test/two")
        } else {
            Reply::bytes(bytes.clone())
        }
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let destination = dir.path().join("out.mp4");

    client(GEMINI_KEY, &shim)
        .save_operation_video(&finished_operation(DOWNLOAD_URL), &destination)
        .expect("download through redirects");

    let requests = server.requests();
    assert_eq!(requests.len(), 3);
    assert!(requests[0].header_contains("x-goog-api-key"));
    for hop in &requests[1..] {
        assert!(
            !hop.header_contains("x-goog-api-key") && !hop.header_contains(GEMINI_KEY),
            "key was forwarded to redirect host {}",
            hop.path
        );
    }
    assert_eq!(
        std::fs::read(&destination).expect("destination"),
        fixture_bytes()
    );
}

#[test]
fn b2_redirect_to_an_allowlisted_host_still_omits_the_key() {
    let bytes = fixture_bytes();
    let server = FakeServer::start(move |_, index| {
        if index == 0 {
            Reply::redirect("https://us-central1-aiplatform.googleapis.com/other")
        } else {
            Reply::bytes(bytes.clone())
        }
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    client(GEMINI_KEY, &shim)
        .save_operation_video(
            &finished_operation(DOWNLOAD_URL),
            &dir.path().join("out.mp4"),
        )
        .expect("download");

    assert!(!server.requests()[1].header_contains("x-goog-api-key"));
}

#[test]
fn b2_redirect_downgrade_and_loops_are_refused() {
    for (location, label) in [
        ("http://insecure.example.test/video.mp4", "downgrade"),
        ("https://loop.example.test/again", "loop"),
    ] {
        let server = FakeServer::start(move |_, _| Reply::redirect(location));
        let shim = CurlShim::new(&server);
        let dir = tempfile::tempdir().expect("tempdir");
        let destination = dir.path().join("out.mp4");

        let result = client(GEMINI_KEY, &shim)
            .save_operation_video(&finished_operation(DOWNLOAD_URL), &destination);

        assert!(
            matches!(result, Err(BakerError::RedirectRefused { .. })),
            "{label}: expected RedirectRefused, got {result:?}"
        );
        assert!(!destination.exists(), "{label}: file must not be created");
        assert!(server.requests().len() <= 6, "{label}: unbounded redirects");
    }
}

#[test]
fn b2_download_uri_outside_the_allowlist_is_refused_before_any_request() {
    for uri in [
        "https://evil.example.test/video.mp4",
        "http://generativelanguage.googleapis.com/video.mp4",
        "https://generativelanguage.googleapis.com.evil.example.test/video.mp4",
        "https://generativelanguage.googleapis.com@evil.example.test/video.mp4",
        "https://generativelanguage.googleapis.com:8443/video.mp4",
        "gs://bucket/video.mp4",
    ] {
        let server = FakeServer::start(|_, _| Reply::bytes(Vec::new()));
        let shim = CurlShim::new(&server);
        let dir = tempfile::tempdir().expect("tempdir");

        let result = client(GEMINI_KEY, &shim)
            .save_operation_video(&finished_operation(uri), &dir.path().join("out.mp4"));

        assert!(
            matches!(result, Err(BakerError::UntrustedHost { .. })),
            "{uri}: expected UntrustedHost, got {result:?}"
        );
        assert!(
            server.requests().is_empty(),
            "{uri}: request must not be sent"
        );
        assert!(shim.stdin().is_empty(), "{uri}: key must not reach curl");
    }
}

#[test]
fn b2_a_key_with_config_syntax_cannot_inject_curl_options() {
    let server = FakeServer::start(|_, _| Reply::json(r#"{"name":"operations/op1"}"#));
    let shim = CurlShim::new(&server);

    let quoted = r#"key"with\quote"#;
    client(quoted, &shim)
        .initiate_generation("prompt", None)
        .expect("quoting must keep a printable key intact");
    assert!(server.requests()[0].header_contains(&format!("x-goog-api-key: {quoted}")));

    let injected = "key\"\nurl = \"https://evil.example.test/";
    let result = client(injected, &shim).initiate_generation("prompt", None);
    assert!(
        matches!(result, Err(BakerError::InvalidApiKey)),
        "expected InvalidApiKey, got {result:?}"
    );
    assert_eq!(server.requests().len(), 1);
}

#[test]
fn b2_debug_output_redacts_the_key() {
    let credentials = VeoCredentials {
        api_key: GEMINI_KEY.to_string(),
        project_id: Some("test-project".to_string()),
    };
    let client = VeoClient::with_credentials(credentials.clone());

    for debug in [format!("{credentials:?}"), format!("{client:?}")] {
        assert!(!debug.contains(GEMINI_KEY), "key in Debug output: {debug}");
        assert!(debug.contains("[REDACTED]"));
        assert!(debug.contains("test-project"));
    }
}

#[test]
fn b2_endpoint_urls_carry_no_key() {
    for key in [GEMINI_KEY, VERTEX_KEY] {
        let client = VeoClient::with_credentials(VeoCredentials {
            api_key: key.to_string(),
            project_id: Some("test-project".to_string()),
        });
        assert!(!client.build_initiate_url().expect("URL").contains(key));
        assert!(!client
            .build_poll_url("operations/op1")
            .expect("URL")
            .contains(key));
    }
}

#[test]
fn b2_api_error_text_never_contains_the_key() {
    let server = FakeServer::start(|_, _| {
        Reply::json(&format!(
            r#"{{"error":{{"message":"bad key {GEMINI_KEY}"}}}}"#
        ))
    });
    let shim = CurlShim::new(&server);

    let error = client(GEMINI_KEY, &shim)
        .initiate_generation("prompt", None)
        .expect_err("API error");

    assert!(!error.to_string().contains(GEMINI_KEY), "{error}");
    assert!(!format!("{error:?}").contains(GEMINI_KEY), "{error:?}");
}

#[test]
fn b2_missing_curl_is_a_typed_error() {
    let client = VeoClient::with_credentials(VeoCredentials {
        api_key: GEMINI_KEY.to_string(),
        project_id: Some("test-project".to_string()),
    })
    .with_curl_path(Path::new("/nonexistent/curl"));

    let result = client.initiate_generation("prompt", None);
    assert!(matches!(result, Err(BakerError::Io { .. })), "{result:?}");
}
