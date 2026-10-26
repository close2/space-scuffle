//! Integration tests for Google Veo client, credentials parsing, and multi-action video baking.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::doc_markdown
)]

use asset_baker::veo_client::{
    build_action_prompt, parse_key_from_api_txt, parse_key_from_env_content,
    parse_project_from_api_txt, parse_project_from_env_content, resolve_credentials, VeoClient,
    VeoCredentials, VeoGenerateRequest, VeoImageInput, VeoInstance, VeoParameters,
};
use asset_baker::{
    bake_multi_action_videos, bake_multi_action_videos_with_states, FlipbookAtlasMeta,
};
use sim_types::enums::AnimState;
use std::path::Path;

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

#[test]
fn test_resolve_credentials_from_workspace() {
    let creds = match resolve_credentials() {
        Ok(c) => c,
        Err(_) => {
            std::env::set_var("GEMINI_API_KEY", "AIzaSyCiFallbackTestKey12345");
            resolve_credentials().expect("Should resolve credentials from env fallback in CI")
        }
    };
    assert_ne!(creds.api_key, "");
    assert_ne!(creds.project_id, "");
}

#[test]
fn test_veo_text_to_video_payload_construction() {
    let prompt = "Aegis Pulse Bot idle animation, StarCraft II portrait aesthetic";
    let req = VeoGenerateRequest {
        instances: vec![VeoInstance {
            prompt: prompt.to_string(),
            image: None,
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
        }],
        parameters: Some(VeoParameters {
            aspect_ratio: Some("16:9".to_string()),
            duration_seconds: Some(8),
            sample_count: Some(1),
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
fn test_veo_client_endpoint_routing() {
    // 1. Vertex AI mode with AQ. key
    let vertex_client = VeoClient::with_credentials(VeoCredentials {
        api_key: "AQ.SecretVertexKey".to_string(),
        project_id: "000000000000".to_string(),
    });
    assert!(vertex_client.is_vertex_ai());
    let vertex_init_url = vertex_client.build_initiate_url();
    assert!(vertex_init_url.contains("us-central1-aiplatform.googleapis.com"));
    assert!(vertex_init_url.contains("projects/000000000000"));
    assert!(vertex_init_url.contains("veo-3.1-fast-generate-001:predictLongRunning"));

    let vertex_poll_url = vertex_client.build_poll_url("operations/12345");
    assert!(vertex_poll_url.contains("fetchPredictOperation"));

    // 2. Gemini Developer API mode with AIza key
    let gemini_client = VeoClient::with_credentials(VeoCredentials {
        api_key: "AIzaSyGeminiKey".to_string(),
        project_id: "000000000000".to_string(),
    });
    assert!(!gemini_client.is_vertex_ai());
    let gemini_init_url = gemini_client.build_initiate_url();
    assert!(gemini_init_url.contains("generativelanguage.googleapis.com"));
    assert!(gemini_init_url.contains("v1beta/models/veo-3.1-fast-generate-001:predictLongRunning"));

    let gemini_poll_url = gemini_client.build_poll_url("operations/abcde");
    assert!(gemini_poll_url.contains("generativelanguage.googleapis.com/v1beta/operations/abcde"));
}

#[test]
fn test_action_prompts_conformity() {
    let pulse_bot_idle = build_action_prompt("pulse_bot", AnimState::Idle);
    assert!(pulse_bot_idle.contains("Pulse Bot"));
    assert!(pulse_bot_idle.contains("Idle animation"));
    assert!(pulse_bot_idle.contains("#0B101B"));

    let psi_grey_attack = build_action_prompt("psi_grey", AnimState::Attack);
    assert!(psi_grey_attack.contains("Psi-Blade Grey"));
    assert!(psi_grey_attack.contains("Attack animation"));

    let rhino_beetle_walk = build_action_prompt("rhino_beetle", AnimState::Walk);
    assert!(rhino_beetle_walk.contains("Rhino-Beetle"));
    assert!(rhino_beetle_walk.contains("Walk animation"));
}

#[test]
fn test_bake_multi_action_videos_with_concept_video() {
    let ffmpeg_path = Path::new("/usr/bin/ffmpeg");
    let test_mp4 = Path::new("tmp/gemini_generated_video_de659ff9.mp4");

    if !ffmpeg_path.exists() || !test_mp4.exists() {
        eprintln!("Skipping test_bake_multi_action_videos: ffmpeg or concept video missing.");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let output_dir = temp_dir.path();

    // Stitch two clips (representing Idle and Walk) using the real concept video
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
    let ffmpeg_path = Path::new("/usr/bin/ffmpeg");
    let test_mp4 = Path::new("tmp/gemini_generated_video_de659ff9.mp4");

    if !ffmpeg_path.exists() || !test_mp4.exists() {
        eprintln!("Skipping test_bake_multi_action_videos_sequential_api: ffmpeg or concept video missing.");
        return;
    }

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
    let ffmpeg_path = Path::new("/usr/bin/ffmpeg");
    let test_mp4 = Path::new("tmp/gemini_generated_video_de659ff9.mp4");

    if !ffmpeg_path.exists() || !test_mp4.exists() {
        eprintln!(
            "Skipping test_multi_action_distinct_frame_offsets: ffmpeg or concept video missing."
        );
        return;
    }

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

    // Invariant: all 6 AnimState variants must be present
    for state in [
        AnimState::Constructing,
        AnimState::Idle,
        AnimState::Walk,
        AnimState::Attack,
        AnimState::Cast,
        AnimState::Die,
    ] {
        assert!(
            meta.actions.contains_key(&state),
            "Manifest must contain fallback mapping for {state:?}"
        );
    }

    // Verify Constructing holds last frame and is non-looping
    let constructing = meta
        .actions
        .get(&AnimState::Constructing)
        .expect("Constructing slice");
    assert!(!constructing.looping);
    assert!(constructing.hold_last_frame);

    // Verify Die holds last frame and is non-looping
    let die = meta.actions.get(&AnimState::Die).expect("Die slice");
    assert!(!die.looping);
    assert!(die.hold_last_frame);
}

#[test]
fn test_multi_action_webgpu_height_bound() {
    let ffmpeg_path = Path::new("/usr/bin/ffmpeg");
    let test_mp4 = Path::new("tmp/gemini_generated_video_de659ff9.mp4");

    if !ffmpeg_path.exists() || !test_mp4.exists() {
        eprintln!(
            "Skipping test_multi_action_webgpu_height_bound: ffmpeg or concept video missing."
        );
        return;
    }

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
