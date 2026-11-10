//! Behavioural tests of the image-model client for effect textures (plan
//! `docs/plans/2026-11-09-effect-visualization.md` §6.2, §6.6; Q0078) without the network.
//!
//! # Responsibility
//! Proves the request shape of `generateContent` (text-to-image and the on-white edit) and of
//! the free `:countTokens` preflight, the response parsing, the key handling of the shared Veo
//! transport, and the paid command's rules: an unpriced model or a run that does not fit is
//! refused before any credential is read, every request is in the ledger, and the budget
//! counts image lines at their model's price while a quota refusal is free.
//!
//! Every request goes through [`common::CurlShim`] to a loopback [`common::FakeServer`]; only
//! obviously fake keys are used.

#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use asset_baker::budget::{BudgetConfig, BUDGET_FILE};
use asset_baker::effects::image_client::{parse_image_response, ImageClient};
use asset_baker::effects::EffectError;
use asset_baker::ledger::LedgerHistory;
use asset_baker::veo_client::{VeoClient, VeoCredentials};
use base64::prelude::*;
use common::{CurlShim, FakeServer, Reply};
use image::{Rgba, RgbaImage};
use serde_json::{json, Value};

mod common;

const VERTEX_KEY: &str = "AQ.FAKE-image-test-only-not-a-credential";
const MODEL: &str = "gemini-3.1-flash-image";

/// A small PNG: a grey disc on `background`.
fn png(background: u8) -> Vec<u8> {
    let image = RgbaImage::from_fn(48, 48, |x, y| {
        let d = (f64::from(x) - 24.0).hypot(f64::from(y) - 24.0);
        if d < 12.0 {
            Rgba([160, 160, 160, 255])
        } else {
            Rgba([background, background, background, 255])
        }
    });
    let mut bytes = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("encode");
    bytes.into_inner()
}

/// A `generateContent` response carrying `bytes` as an inline image.
fn image_reply(bytes: &[u8]) -> String {
    json!({"candidates": [{"content": {"role": "model", "parts": [
        {"inlineData": {"mimeType": "image/png", "data": BASE64_STANDARD.encode(bytes)}}
    ]}, "finishReason": "STOP"}]})
    .to_string()
}

fn client(shim: &CurlShim, location: &str) -> ImageClient {
    let transport = VeoClient::with_credentials(VeoCredentials {
        api_key: VERTEX_KEY.to_string(),
        project_id: Some("test-project".to_string()),
    })
    .with_model(MODEL)
    .with_curl_path(shim.path());
    ImageClient::new(transport, location, "1K").expect("client")
}

fn body(request: &common::ReceivedRequest) -> Value {
    serde_json::from_slice(&request.body).expect("JSON body")
}

#[test]
fn a_text_to_image_request_has_the_documented_shape_and_returns_the_image() {
    let picture = png(0);
    let reply = image_reply(&picture);
    let server = FakeServer::start(move |_, _| Reply::json(&reply));
    let shim = CurlShim::new(&server);

    let image = client(&shim, "us-central1")
        .generate("an isolated smoke puff on pure black")
        .expect("image");
    assert_eq!(image.bytes, picture);

    let requests = server.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST");
    assert_eq!(
        requests[0].path,
        "/us-central1-aiplatform.googleapis.com/v1/projects/test-project/locations/us-central1/publishers/google/models/gemini-3.1-flash-image:generateContent"
    );
    let sent = body(&requests[0]);
    assert_eq!(sent["contents"][0]["role"], "user");
    assert_eq!(
        sent["contents"][0]["parts"][0]["text"],
        "an isolated smoke puff on pure black"
    );
    assert_eq!(
        sent["generationConfig"]["responseModalities"],
        json!(["IMAGE"])
    );
    assert_eq!(
        sent["generationConfig"]["imageConfig"]["aspectRatio"],
        "1:1"
    );
    assert_eq!(sent["generationConfig"]["imageConfig"]["imageSize"], "1K");
    assert!(requests[0].header_contains(&format!("x-goog-api-key: {VERTEX_KEY}")));
    assert!(
        !shim.argv().contains(VERTEX_KEY),
        "the key must not be in argv"
    );
    assert!(!requests[0].path.contains(VERTEX_KEY));
}

#[test]
fn the_edit_request_sends_the_picture_inline_before_the_text() {
    let white = png(255);
    let reply = image_reply(&white);
    let server = FakeServer::start(move |_, _| Reply::json(&reply));
    let shim = CurlShim::new(&server);
    let black = png(0);

    let image = client(&shim, "global")
        .edit("the same element on pure white", &black)
        .expect("image");
    assert_eq!(image.bytes, white);

    let requests = server.requests();
    assert_eq!(
        requests[0].path,
        "/aiplatform.googleapis.com/v1/projects/test-project/locations/global/publishers/google/models/gemini-3.1-flash-image:generateContent"
    );
    let parts = &body(&requests[0])["contents"][0]["parts"];
    assert_eq!(parts[0]["inlineData"]["mimeType"], "image/png");
    assert_eq!(
        parts[0]["inlineData"]["data"],
        BASE64_STANDARD.encode(&black)
    );
    assert_eq!(parts[1]["text"], "the same element on pure white");
}

#[test]
fn the_preflight_counts_tokens_and_reports_a_refusal_without_failing() {
    let server = FakeServer::start(|request, _| {
        if request.path.contains("locations/global") {
            Reply::status(
                403,
                r#"{"error":{"code":403,"message":"denied","status":"PERMISSION_DENIED"}}"#,
            )
        } else {
            Reply::json(r#"{"totalTokens": 1}"#)
        }
    });
    let shim = CurlShim::new(&server);

    let ok = client(&shim, "us-central1").preflight().expect("report");
    assert!(ok.callable());
    assert!(
        ok.url.ends_with("gemini-3.1-flash-image:countTokens"),
        "{}",
        ok.url
    );
    let refused = client(&shim, "global").preflight().expect("report");
    assert!(!refused.callable());
    assert_eq!(refused.http_status, 403);
    assert_eq!(refused.error_status.as_deref(), Some("PERMISSION_DENIED"));
    let requests = server.requests();
    assert!(requests.iter().all(|r| r.path.ends_with(":countTokens")));
    assert!(body(&requests[0])["contents"][0]["parts"][0]["text"].is_string());
}

#[test]
fn responses_in_snake_case_parse_and_untrusted_locations_are_refused() {
    let picture = png(0);
    let snake = json!({"candidates": [{"content": {"parts": [
        {"text": "here"},
        {"inline_data": {"mime_type": "image/png", "data": BASE64_STANDARD.encode(&picture)}}
    ]}}]});
    assert_eq!(
        parse_image_response(&snake, &|t| t.to_string())
            .expect("image")
            .bytes,
        picture
    );

    let server = FakeServer::start(|_, _| Reply::json("{}"));
    let shim = CurlShim::new(&server);
    let error = client(&shim, "europe-west4")
        .generate("p")
        .expect_err("untrusted host");
    assert!(error.to_string().contains("untrusted"), "{error}");
    assert!(server.requests().is_empty());
    let transport = VeoClient::with_credentials(VeoCredentials {
        api_key: VERTEX_KEY.to_string(),
        project_id: Some("p".to_string()),
    })
    .with_model("bad/model");
    assert!(matches!(
        ImageClient::new(transport, "us-central1", "1K"),
        Err(EffectError::InvalidRecipe { .. })
    ));
}

/// The paid command with the fake Vertex key, the shim and the shipped bundle.
fn effect_image(dirs: &std::path::Path, shim: &CurlShim, extra: &[&str]) -> std::process::Output {
    let credentials = common::cli::fake_vertex_credentials(dirs);
    common::cli::baker()
        .args([
            "effect-image",
            "--set",
            "fx_test",
            "--element",
            "smoke_puff",
            "--description",
            "A dense billowing smoke puff.",
        ])
        .args(extra)
        .arg("--ledger")
        .arg(dirs.join("ledger.jsonl"))
        .arg("--out-dir")
        .arg(dirs.join("images"))
        .arg("--credentials-dir")
        .arg(credentials)
        .arg("--curl-path")
        .arg(shim.path())
        .output()
        .expect("run baker")
}

fn ledger_lines(dir: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join("ledger.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).expect("ledger line"))
        .collect()
}

#[test]
fn an_unpriced_model_or_a_run_that_does_not_fit_is_refused_before_any_credential() {
    let server = FakeServer::start(|_, _| Reply::json("{}"));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let trap = common::cli::trapped_credentials(dir.path());
    for (model, max) in [("gemini-unpriced-image", "2"), (MODEL, "1")] {
        let output = common::cli::baker()
            .args([
                "effect-image",
                "--set",
                "fx_test",
                "--element",
                "smoke_puff",
                "--description",
                "A smoke puff.",
                "--pair",
                "--model",
                model,
                "--max-generations",
                max,
            ])
            .arg("--ledger")
            .arg(dir.path().join("ledger.jsonl"))
            .arg("--out-dir")
            .arg(dir.path().join("images"))
            .arg("--credentials-dir")
            .arg(&trap)
            .arg("--curl-path")
            .arg(shim.path())
            .output()
            .expect("run");
        let text = common::cli::text(&output);
        assert!(!output.status.success(), "{text}");
        assert!(
            text.contains("has no price") || text.contains("--max-generations is 1"),
            "{text}"
        );
    }
    assert!(server.requests().is_empty());
    assert!(!dir.path().join("ledger.jsonl").exists());
}

#[test]
fn a_paid_pair_writes_both_images_and_four_ledger_lines_counted_at_the_model_price() {
    let black = png(0);
    let white = png(255);
    let replies = [image_reply(&black), image_reply(&white)];
    let server = FakeServer::start(move |_, index| Reply::json(&replies[index.min(1)]));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    let output = effect_image(
        dir.path(),
        &shim,
        &["--pair", "--model", MODEL, "--max-generations", "2"],
    );
    let text = common::cli::text(&output);
    assert!(output.status.success(), "{text}");
    common::cli::assert_no_key(&text, "the command output");

    let lines = ledger_lines(dir.path());
    let events: Vec<&str> = lines
        .iter()
        .map(|l| l["event"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        events,
        [
            "image_request_started",
            "image_downloaded",
            "image_request_started",
            "image_downloaded"
        ]
    );
    assert_eq!(lines[0]["kind"], "on_black");
    assert_eq!(lines[0]["model"], MODEL);
    assert_eq!(lines[2]["kind"], "on_white_edit");
    assert_eq!(lines[2]["source_sha256"], lines[1]["sha256"]);
    for line in [&lines[1], &lines[3]] {
        let path = line["path"].as_str().expect("path");
        assert_eq!(
            asset_baker::ledger::sha256_file(std::path::Path::new(path)).expect("file"),
            line["sha256"].as_str().expect("sha")
        );
    }
    let edit = body(&server.requests()[1]);
    assert_eq!(
        edit["contents"][0]["parts"][0]["inlineData"]["data"],
        BASE64_STANDARD.encode(&black)
    );
    let ledger_text = std::fs::read_to_string(dir.path().join("ledger.jsonl")).expect("ledger");
    common::cli::assert_no_key(&ledger_text, "the ledger");

    let budget = BudgetConfig::load(&common::cli::shipped_assets().join(BUDGET_FILE))
        .expect("shipped budget");
    let history = LedgerHistory::load(&dir.path().join("ledger.jsonl")).expect("history");
    assert_eq!(budget.spent_cents(&history).expect("priced"), 22);
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

    let output = effect_image(
        dir.path(),
        &shim,
        &["--model", MODEL, "--max-generations", "1"],
    );
    assert_eq!(
        output.status.code(),
        Some(3),
        "{}",
        common::cli::text(&output)
    );
    let lines = ledger_lines(dir.path());
    let events: Vec<&str> = lines
        .iter()
        .map(|l| l["event"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(events, ["image_request_started", "image_quota_refused"]);
    assert_eq!(server.requests().len(), 1, "a quota refusal is not retried");
    let budget = BudgetConfig::load(&common::cli::shipped_assets().join(BUDGET_FILE))
        .expect("shipped budget");
    let history = LedgerHistory::load(&dir.path().join("ledger.jsonl")).expect("history");
    assert_eq!(budget.spent_cents(&history).expect("priced"), 0);
}

#[test]
fn a_response_without_an_image_is_recorded_as_a_paid_failure() {
    let server =
        FakeServer::start(|_, _| Reply::json(r#"{"promptFeedback": {"blockReason": "SAFETY"}}"#));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    let output = effect_image(
        dir.path(),
        &shim,
        &["--model", MODEL, "--max-generations", "1"],
    );
    let text = common::cli::text(&output);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(text.contains("SAFETY"), "{text}");
    let lines = ledger_lines(dir.path());
    assert_eq!(lines[1]["event"], "image_request_failed");
    let budget = BudgetConfig::load(&common::cli::shipped_assets().join(BUDGET_FILE))
        .expect("shipped budget");
    let history = LedgerHistory::load(&dir.path().join("ledger.jsonl")).expect("history");
    assert_eq!(budget.spent_cents(&history).expect("priced"), 11);
}

#[test]
fn the_preflight_command_prints_the_status_and_fails_when_the_model_is_not_callable() {
    let server = FakeServer::start(|_, _| {
        Reply::status(
            404,
            r#"{"error":{"code":404,"status":"NOT_FOUND","message":"no"}}"#,
        )
    });
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");
    let credentials = common::cli::fake_vertex_credentials(dir.path());
    let output = common::cli::baker()
        .args([
            "effect-image-preflight",
            "--model",
            MODEL,
            "--credentials-dir",
        ])
        .arg(credentials)
        .arg("--curl-path")
        .arg(shim.path())
        .output()
        .expect("run");
    let text = common::cli::text(&output);
    assert!(!output.status.success(), "{text}");
    assert!(
        text.contains("HTTP status: 404") && text.contains("NOT_FOUND"),
        "{text}"
    );
    common::cli::assert_no_key(&text, "the preflight output");
    assert!(server.requests()[0].path.ends_with(":countTokens"));
}

#[test]
fn an_edit_only_request_completes_a_recorded_picture_and_refuses_an_unrecorded_one() {
    let black = png(0);
    let white = png(255);
    let replies = [image_reply(&black), image_reply(&white)];
    let server = FakeServer::start(move |_, index| Reply::json(&replies[index.min(1)]));
    let shim = CurlShim::new(&server);
    let dir = tempfile::tempdir().expect("tempdir");

    // The picture on black of an earlier run whose edit was refused.
    let first = effect_image(
        dir.path(),
        &shim,
        &["--model", MODEL, "--max-generations", "1"],
    );
    assert!(first.status.success(), "{}", common::cli::text(&first));
    let lines = ledger_lines(dir.path());
    let black_path = lines[1]["path"].as_str().expect("path").to_string();

    let output = effect_image(
        dir.path(),
        &shim,
        &[
            "--edit-of",
            &black_path,
            "--model",
            MODEL,
            "--max-generations",
            "1",
        ],
    );
    let text = common::cli::text(&output);
    assert!(output.status.success(), "{text}");
    let lines = ledger_lines(dir.path());
    assert_eq!(lines.len(), 4);
    assert_eq!(lines[2]["event"], "image_request_started");
    assert_eq!(lines[2]["kind"], "on_white_edit");
    assert_eq!(lines[2]["source_sha256"], lines[1]["sha256"]);
    assert_eq!(lines[3]["event"], "image_downloaded");
    let edit = body(&server.requests()[1]);
    assert_eq!(
        edit["contents"][0]["parts"][0]["inlineData"]["data"],
        BASE64_STANDARD.encode(&black)
    );
    let budget = BudgetConfig::load(&common::cli::shipped_assets().join(BUDGET_FILE))
        .expect("shipped budget");
    let history = LedgerHistory::load(&dir.path().join("ledger.jsonl")).expect("history");
    assert_eq!(budget.spent_cents(&history).expect("priced"), 22);

    // A picture the ledger never recorded for this element is refused before any request.
    let stranger = dir.path().join("stranger.png");
    std::fs::write(&stranger, png(10)).expect("png");
    let refused = effect_image(
        dir.path(),
        &shim,
        &[
            "--edit-of",
            stranger.to_str().expect("utf-8"),
            "--model",
            MODEL,
            "--max-generations",
            "1",
        ],
    );
    assert!(!refused.status.success());
    assert_eq!(server.requests().len(), 2);
    assert_eq!(ledger_lines(dir.path()).len(), 4);
}
