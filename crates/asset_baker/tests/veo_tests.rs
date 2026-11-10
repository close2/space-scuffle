//! Integration tests for Google Veo client, credentials parsing, and multi-action video baking.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use asset_baker::prompts::{build_action_prompt, MediaStyle};
use asset_baker::veo_client::{
    encode_image, parse_key_from_api_txt, parse_key_from_env_content, parse_project_from_api_txt,
    parse_project_from_env_content, resolve_credentials_from, GenerationImages, VeoClient,
    VeoCredentials, VeoGenerateRequest, VeoImageInput, VeoInstance, VeoParameters,
};
use asset_baker::{
    bake_custom_video, bake_multi_action_videos, bake_multi_action_videos_with_states, BakerError,
    FlipbookAtlasMeta,
};
use sim_types::enums::AnimState;
use std::path::Path;

/// The shipped unit catalog; the baker itself loads it from disk at run time.
fn catalog() -> sim_core::UnitCatalog {
    sim_fixtures::shipped_catalogs().units
}

/// The shipped media style (`assets/catalog/media_style.ron`).
fn style() -> MediaStyle {
    MediaStyle::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
        .expect("shipped media style")
}

mod common;

#[test]
fn test_env_content_key_and_project_parsing() {
    let env_sample = r#"
# Space Scuffle Secrets
GEMINI_API_KEY="AIzaSyTestKey123456789"
GOOGLE_CLOUD_PROJECT=000000000000
"#;
    let key = parse_key_from_env_content(env_sample).expect("Failed to parse GEMINI_API_KEY");
    assert_eq!(key, "AIzaSyTestKey123456789");

    let proj =
        parse_project_from_env_content(env_sample).expect("Failed to parse GOOGLE_CLOUD_PROJECT");
    assert_eq!(proj, "000000000000");

    let export_sample = r#"
export GOOGLE_API_KEY='AQ.VertexExpressKey987'
export GOOGLE_CLOUD_PROJECT="custom-project-42"
"#;
    let key2 = parse_key_from_env_content(export_sample).expect("Failed to parse GOOGLE_API_KEY");
    assert_eq!(key2, "AQ.VertexExpressKey987");

    let proj2 =
        parse_project_from_env_content(export_sample).expect("Failed to parse custom project");
    assert_eq!(proj2, "custom-project-42");
}

#[test]
fn test_api_txt_content_parsing() {
    let txt_sample = r"
Gemini API Key: AQ.TestVertexExpressMockKey000000000000
name: antigravity
project name: projects/000000000000
project number: 000000000000
";
    let key = parse_key_from_api_txt(txt_sample).expect("Failed to parse key from api.txt format");
    assert_eq!(key, "AQ.TestVertexExpressMockKey000000000000");

    let proj = parse_project_from_api_txt(txt_sample).expect("Failed to parse project number");
    assert_eq!(proj, "000000000000");
}

const FAKE_GEMINI_KEY: &str = "AIzaFAKE-test-only-not-a-credential";
const FAKE_VERTEX_KEY: &str = "AQ.FAKE-test-only-not-a-credential";

/// A credential source with nothing in it, for tests that use only their temporary files.
fn empty_environment(_: &str) -> Option<String> {
    None
}

/// Reads credentials from a temporary root only. Neither the process environment nor any file
/// outside the root is consulted, so running this suite never touches a developer's real key.
fn resolve_in(root: &Path) -> Result<VeoCredentials, BakerError> {
    resolve_credentials_from(empty_environment, root)
}

#[test]
fn b5_vertex_key_without_a_project_is_a_typed_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::write(
        root.join(".env"),
        format!("GOOGLE_API_KEY={FAKE_VERTEX_KEY}\n"),
    )
    .expect("env");

    let result = resolve_in(&root);

    assert!(
        matches!(result, Err(BakerError::ProjectIdMissing)),
        "a placeholder project must not be substituted: {result:?}"
    );
}

#[test]
fn b5_gemini_key_does_not_need_a_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::write(
        root.join(".env"),
        format!("GEMINI_API_KEY={FAKE_GEMINI_KEY}\n"),
    )
    .expect("env");

    let credentials = resolve_in(&root).expect("Gemini credentials");

    assert_eq!(credentials.api_key, FAKE_GEMINI_KEY);
    assert_eq!(credentials.project_id, None);
}

#[test]
fn b5_environment_lookup_takes_priority_over_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).expect("root");
    std::fs::write(root.join(".env"), "GEMINI_API_KEY=AIzaFromFile\n").expect("env");

    let credentials = resolve_credentials_from(
        |name| match name {
            "GOOGLE_API_KEY" => Some(FAKE_VERTEX_KEY.to_string()),
            "GOOGLE_CLOUD_PROJECT" => Some("from-environment".to_string()),
            _ => None,
        },
        &root,
    )
    .expect("credentials");

    assert_eq!(credentials.api_key, FAKE_VERTEX_KEY);
    assert_eq!(credentials.project_id.as_deref(), Some("from-environment"));
}

#[test]
fn b5_key_and_project_are_found_in_api_key_txt_and_in_the_parent_directory() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(root.join("crate")).expect("root");
    std::fs::create_dir_all(root.join("tmp")).expect("tmp");
    std::fs::write(
        root.join("tmp/api-key.txt"),
        format!("Gemini API Key: {FAKE_VERTEX_KEY}\nproject number: 123\n"),
    )
    .expect("key file");

    let credentials = resolve_in(&root.join("crate")).expect("credentials");

    assert_eq!(credentials.api_key, FAKE_VERTEX_KEY);
    assert_eq!(credentials.project_id.as_deref(), Some("123"));
}

#[test]
fn b5_missing_key_is_reported_and_no_outside_location_is_consulted() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(&root).expect("root");

    let result = resolve_in(&root);

    assert!(
        matches!(result, Err(BakerError::ApiKeyMissing)),
        "{result:?}"
    );
}

#[test]
fn b5_unreadable_credential_file_is_not_silently_ignored() {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().join("workspace");
    std::fs::create_dir_all(root.join(".env")).expect("a directory where the file should be");

    let result = resolve_in(&root);

    assert!(matches!(result, Err(BakerError::Io { .. })), "{result:?}");
}

#[test]
fn b5_no_source_file_contains_a_developer_home_path() {
    // Every file under src/, including module directories such as src/effects/.
    let mut pending = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).expect("read src") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                files.push(path);
            }
        }
    }
    assert!(files.len() > 10, "the walk found the sources");
    for path in files {
        let text = std::fs::read_to_string(&path).expect("read source");
        assert!(
            !text.contains("/home/"),
            "{} has a home path",
            path.display()
        );
    }
}

#[test]
fn test_veo_text_to_video_payload_construction() {
    let prompt = "Aegis Pulse Bot idle animation, StarCraft II portrait aesthetic";
    let req = VeoGenerateRequest {
        instances: vec![VeoInstance {
            prompt: prompt.to_string(),
            image: None,
            last_frame: None,
        }],
        parameters: Some(VeoParameters::default()),
    };

    let json_val = serde_json::to_value(&req).expect("Serialization failed");
    assert_eq!(json_val["instances"][0]["prompt"], prompt);
    assert!(json_val["instances"][0].get("image").is_none());
    assert_eq!(json_val["parameters"]["aspectRatio"], "16:9");
    assert_eq!(json_val["parameters"]["durationSeconds"], 4);
    assert_eq!(json_val["parameters"]["sampleCount"], 1);
}

#[test]
fn test_veo_image_to_video_payload_construction() {
    let prompt = "Aegis Pulse Bot attack firing animation";
    let fake_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    let req = VeoGenerateRequest {
        instances: vec![VeoInstance {
            prompt: prompt.to_string(),
            image: Some(VeoImageInput {
                bytes_base64_encoded: fake_b64.to_string(),
                mime_type: "image/png".to_string(),
            }),
            last_frame: None,
        }],
        parameters: Some(VeoParameters {
            aspect_ratio: Some("16:9".to_string()),
            duration_seconds: Some(8),
            sample_count: Some(1),
            generate_audio: None,
            negative_prompt: None,
            seed: None,
        }),
    };

    let json_val = serde_json::to_value(&req).expect("Serialization failed");
    assert_eq!(json_val["instances"][0]["prompt"], prompt);
    assert_eq!(
        json_val["instances"][0]["image"]["bytesBase64Encoded"],
        fake_b64
    );
    assert_eq!(json_val["instances"][0]["image"]["mimeType"], "image/png");
    assert_eq!(json_val["parameters"]["durationSeconds"], 8);
}

#[test]
fn loops_an_end_frame_is_sent_as_last_frame_beside_the_start_frame() {
    let picture = VeoImageInput {
        bytes_base64_encoded: "cGlu".to_string(),
        mime_type: "image/png".to_string(),
    };
    let req = VeoGenerateRequest {
        instances: vec![VeoInstance {
            prompt: "a loop".to_string(),
            image: Some(picture.clone()),
            last_frame: Some(picture),
        }],
        parameters: Some(VeoParameters::default()),
    };

    let json = serde_json::to_value(&req).expect("Serialization failed");

    let instance = &json["instances"][0];
    assert_eq!(instance["image"], instance["lastFrame"]);
    assert_eq!(instance["lastFrame"]["bytesBase64Encoded"], "cGlu");
    assert_eq!(instance["lastFrame"]["mimeType"], "image/png");
    assert!(instance.get("last_frame").is_none(), "{instance}");
}

/// A client whose curl does not exist: any request it tried to send would fail with an I/O
/// error, so a typed refusal proves that nothing was sent.
fn offline_client(api_key: &str) -> VeoClient {
    VeoClient::with_credentials(VeoCredentials {
        api_key: api_key.to_string(),
        project_id: Some("fixture-project".to_string()),
    })
    .with_curl_path(Path::new("/nonexistent/curl-must-not-run"))
}

#[test]
fn loops_end_frames_are_refused_without_a_start_frame_or_outside_vertex() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pin = dir.path().join("pin.png");
    image::RgbImage::new(16, 9).save(&pin).expect("png");

    let without_start = offline_client(FAKE_VERTEX_KEY).initiate_generation(
        "prompt",
        GenerationImages {
            start: None,
            end: Some(&pin),
        },
    );
    assert!(
        matches!(without_start, Err(BakerError::LastFrameWithoutStart)),
        "{without_start:?}"
    );

    let gemini = offline_client(FAKE_GEMINI_KEY).initiate_generation(
        "prompt",
        GenerationImages {
            start: Some(&pin),
            end: Some(&pin),
        },
    );
    assert!(
        matches!(gemini, Err(BakerError::LastFrameNeedsVertex { .. })),
        "{gemini:?}"
    );
}

#[test]
fn loops_the_mime_type_is_read_from_the_bytes_and_other_formats_are_refused() {
    let dir = tempfile::tempdir().expect("tempdir");
    let png_named_jpg = dir.path().join("picture.jpg");
    image::RgbImage::new(16, 9)
        .save_with_format(&png_named_jpg, image::ImageFormat::Png)
        .expect("png");
    let jpeg_named_png = dir.path().join("picture.png");
    image::RgbImage::new(16, 9)
        .save_with_format(&jpeg_named_png, image::ImageFormat::Jpeg)
        .expect("jpeg");
    let gif = dir.path().join("picture.gif");
    std::fs::write(&gif, b"GIF89a\x01\x00\x01\x00").expect("gif");

    assert_eq!(
        encode_image(&png_named_jpg).expect("png").mime_type,
        "image/png"
    );
    assert_eq!(
        encode_image(&jpeg_named_png).expect("jpeg").mime_type,
        "image/jpeg"
    );
    assert!(matches!(
        encode_image(&gif),
        Err(BakerError::UnsupportedImageFormat { .. })
    ));
    let refused = offline_client(FAKE_VERTEX_KEY).initiate_generation(
        "prompt",
        GenerationImages {
            start: Some(&gif),
            end: None,
        },
    );
    assert!(
        matches!(refused, Err(BakerError::UnsupportedImageFormat { .. })),
        "{refused:?}"
    );
}

#[test]
fn test_veo_client_endpoint_routing() {
    // 1. Vertex AI mode with AQ. key
    let vertex_client = VeoClient::with_credentials(VeoCredentials {
        api_key: "AQ.SecretVertexKey".to_string(),
        project_id: Some("000000000000".to_string()),
    });
    assert!(vertex_client.is_vertex_ai());
    let vertex_init_url = vertex_client.build_initiate_url().expect("URL");
    assert!(vertex_init_url.contains("us-central1-aiplatform.googleapis.com"));
    assert!(vertex_init_url.contains("projects/000000000000"));
    assert!(vertex_init_url.contains("veo-3.1-fast-generate-001:predictLongRunning"));

    let vertex_poll_url = vertex_client
        .build_poll_url("operations/12345")
        .expect("URL");
    assert!(vertex_poll_url.contains("fetchPredictOperation"));

    // 2. Gemini Developer API mode with AIza key
    let gemini_client = VeoClient::with_credentials(VeoCredentials {
        api_key: "AIzaSyGeminiKey".to_string(),
        project_id: Some("000000000000".to_string()),
    });
    assert!(!gemini_client.is_vertex_ai());
    let gemini_init_url = gemini_client.build_initiate_url().expect("URL");
    assert!(gemini_init_url.contains("generativelanguage.googleapis.com"));
    assert!(gemini_init_url.contains("v1beta/models/veo-3.1-fast-generate-001:predictLongRunning"));

    let gemini_poll_url = gemini_client
        .build_poll_url("operations/abcde")
        .expect("URL");
    assert!(gemini_poll_url.contains("generativelanguage.googleapis.com/v1beta/operations/abcde"));
}

#[test]
fn test_action_prompts_conformity() {
    let pulse_bot_idle = build_action_prompt(&catalog(), &style(), "pulse_bot", AnimState::Idle)
        .expect("catalog slug");
    assert!(pulse_bot_idle.authored);
    let pulse_bot_idle = pulse_bot_idle.text;
    assert!(pulse_bot_idle.contains("Pulse Bot"));
    assert!(pulse_bot_idle.contains("Idle animation"));
    assert!(pulse_bot_idle.contains("#0B101B"));

    let psi_grey_attack = build_action_prompt(&catalog(), &style(), "psi_grey", AnimState::Attack)
        .expect("catalog slug")
        .text;
    assert!(psi_grey_attack.contains("Psi-Blade Grey"));
    assert!(psi_grey_attack.contains("Attack animation"));

    let rhino_beetle_walk =
        build_action_prompt(&catalog(), &style(), "rhino_beetle", AnimState::Walk)
            .expect("catalog slug")
            .text;
    assert!(rhino_beetle_walk.contains("Rhino-Beetle"));
    assert!(rhino_beetle_walk.contains("Walk animation"));
}

#[test]
fn test_bake_multi_action_videos_with_concept_video() {
    let clip = common::fixture_clip();
    let test_mp4 = clip.as_path();

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let output_dir = temp_dir.path();

    // Stitch two clips (representing Idle and Walk) using the real fixture clip
    let clips = [(AnimState::Idle, test_mp4), (AnimState::Walk, test_mp4)];

    let meta = bake_multi_action_videos_with_states("test_unit", &clips, output_dir, 128, 5)
        .expect("bake_multi_action_videos_with_states failed");

    assert_eq!(meta.name, "test_unit");
    assert_eq!(meta.frame_width, 128);
    assert_eq!(meta.frame_height, 128);
    assert_eq!(meta.fps, 5);
    assert!(meta.total_frames > 0);

    // Verify ActionSlices
    let idle_slice = meta
        .actions
        .get(&AnimState::Idle)
        .expect("Missing Idle slice");
    assert_eq!(idle_slice.start_frame, 0);
    assert!(idle_slice.frame_count > 0);
    assert!(idle_slice.looping);

    let walk_slice = meta
        .actions
        .get(&AnimState::Walk)
        .expect("Missing Walk slice");
    assert_eq!(walk_slice.start_frame, idle_slice.frame_count);
    assert!(walk_slice.frame_count > 0);
    assert!(walk_slice.looping);

    assert_eq!(
        meta.total_frames,
        idle_slice.frame_count + walk_slice.frame_count
    );

    // Verify written files on disk
    let png_path = output_dir.join("test_unit.png");
    let ron_path = output_dir.join("test_unit.flipbook.ron");

    assert!(png_path.exists(), "PNG strip must exist on disk");
    assert!(ron_path.exists(), "RON manifest must exist on disk");

    let loaded_meta = FlipbookAtlasMeta::load_from_ron(&ron_path)
        .expect("Failed to deserialize saved RON manifest");
    assert_eq!(loaded_meta.name, meta.name);
    assert_eq!(loaded_meta.total_frames, meta.total_frames);
}

#[test]
fn test_bake_multi_action_videos_sequential_api() {
    let clip = common::fixture_clip();
    let test_mp4 = clip.as_path();

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let output_dir = temp_dir.path();

    // Call the exact signature: bake_multi_action_videos(unit_name, &[idle, walk, attack], output_dir, frame_size, fps)
    let meta = bake_multi_action_videos("marine", &[test_mp4, test_mp4], output_dir, 128, 5)
        .expect("bake_multi_action_videos failed");

    assert_eq!(meta.name, "marine");
    assert!(meta.actions.contains_key(&AnimState::Idle));
    assert!(meta.actions.contains_key(&AnimState::Walk));
}

#[test]
fn test_multi_action_distinct_frame_offsets() {
    let clip = common::fixture_clip();
    let test_mp4 = clip.as_path();

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let output_dir = temp_dir.path();

    let clips = [
        (AnimState::Idle, test_mp4),
        (AnimState::Walk, test_mp4),
        (AnimState::Attack, test_mp4),
    ];

    let meta = bake_multi_action_videos_with_states("pulse_bot", &clips, output_dir, 256, 10)
        .expect("bake_multi_action_videos_with_states failed");

    let idle = meta.actions.get(&AnimState::Idle).expect("Idle slice");
    let walk = meta.actions.get(&AnimState::Walk).expect("Walk slice");
    let attack = meta.actions.get(&AnimState::Attack).expect("Attack slice");

    // Invariant: start_frame offsets must be distinct and non-overlapping
    assert_eq!(idle.start_frame, 0);
    assert_eq!(walk.start_frame, idle.start_frame + idle.frame_count);
    assert_eq!(attack.start_frame, walk.start_frame + walk.frame_count);
    assert!(idle.frame_count > 0);
    assert!(walk.frame_count > 0);
    assert!(attack.frame_count > 0);

    // Invariant: only the supplied actions are listed
    for state in [AnimState::Constructing, AnimState::Cast, AnimState::Die] {
        assert!(
            !meta.actions.contains_key(&state),
            "Manifest must not invent an action for {state:?}"
        );
    }
}

#[test]
fn test_multi_action_webgpu_height_bound() {
    let clip = common::fixture_clip();
    let test_mp4 = clip.as_path();

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let output_dir = temp_dir.path();

    // 3 clips at 512x512 resolution
    let clips = [
        (AnimState::Idle, test_mp4),
        (AnimState::Walk, test_mp4),
        (AnimState::Attack, test_mp4),
    ];

    let meta = bake_multi_action_videos_with_states("psi_grey", &clips, output_dir, 512, 15)
        .expect("bake_multi_action_videos_with_states at 512px failed");

    // Budget math: max_total = 16384 / 512 = 32. 32 / 3 = 10 frames per clip.
    // Total frames must be <= 30 frames.
    assert!(
        meta.total_frames <= 30,
        "Total frames {} exceeds 30-frame budget for 3 clips at 512px",
        meta.total_frames
    );

    let strip_height = meta.total_frames * 512;
    assert!(
        strip_height <= 16384,
        "Strip height {strip_height} exceeds WebGPU hardware limit of 16384 px"
    );

    let png_path = output_dir.join("psi_grey.png");
    let img = image::open(&png_path).expect("Failed to open baked PNG");
    assert_eq!(img.width(), 512);
    assert_eq!(img.height(), strip_height);
    assert!(img.height() <= 16384);
}

#[test]
fn b9_traversal_in_an_atlas_name_cannot_escape_the_output_directory() {
    let clip = common::fixture_clip();
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("deep/output");

    let multi = bake_multi_action_videos_with_states(
        "../../escaped",
        &[(AnimState::Idle, clip.as_path())],
        &output,
        32,
        5,
    );
    let single = bake_custom_video(&clip, &output, "../../escaped", 32, 5);

    assert!(multi.is_err(), "{multi:?}");
    assert!(single.is_err(), "{single:?}");
    assert!(!dir.path().join("escaped.png").exists());
}

#[test]
fn b5_vertex_endpoints_refuse_a_missing_project_instead_of_substituting_one() {
    let client = VeoClient::with_credentials(VeoCredentials {
        api_key: FAKE_VERTEX_KEY.to_string(),
        project_id: None,
    });

    assert!(matches!(
        client.build_initiate_url(),
        Err(BakerError::ProjectIdMissing)
    ));
    assert!(matches!(
        client.build_poll_url("operations/op"),
        Err(BakerError::ProjectIdMissing)
    ));
}

#[test]
fn b9_prompts_exist_only_for_exact_catalog_slugs() {
    for unit in [
        "fictional_unit",
        "Pulse-Bot",
        "PULSE_BOT",
        "Pulse Bot",
        "pulse",
    ] {
        assert!(
            matches!(
                build_action_prompt(&catalog(), &style(), unit, AnimState::Idle),
                Err(BakerError::UnknownArchetype { .. })
            ),
            "{unit:?} must not receive a prompt"
        );
    }
}
