//! Command-line interface for the Space Scuffle asset baker.
//!
//! Provides CLI commands to generate authentic video-backed flipbook atlases using
//! Google Veo video generation models, transcode multi-action videos via FFmpeg,
//! or synthesize procedural vector fallback atlases.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting
)]

use clap::{Parser, Subcommand};
use sim_types::enums::AnimState;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "asset_baker")]
#[command(author = "Space Scuffle Systems Engineering Team")]
#[command(version = "0.1.0")]
#[command(about = "Space Scuffle video and procedural flipbook atlas baker")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Generates high-quality flipbook atlases using real video frames from the concept video.
    GenerateDefaults {
        /// Target output directory for baked PNG strips and .flipbook.ron manifests.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,

        /// Path to the concept MP4 video to bake into starter unit flipbook atlases.
        #[arg(
            long = "concept-mp4",
            alias = "concept-video",
            default_value = "tmp/gemini_generated_video_de659ff9.mp4"
        )]
        concept_video: PathBuf,

        /// Target frame size (default: 512).
        #[arg(short, long, default_value_t = 512)]
        size: u32,

        /// Target framerate (default: 15).
        #[arg(short, long, default_value_t = 15)]
        fps: u32,
    },

    /// Generates genuine 3D animation videos using Google Veo video generation models.
    #[command(name = "generate-veo")]
    GenerateVeo {
        /// Specific unit archetype to generate (e.g. "marine", "marauder", "zealot", "command_dome").
        #[arg(short, long)]
        unit: Option<String>,

        /// Flag to generate all 4 starter archetypes: marine, zealot, marauder, command_dome.
        #[arg(long)]
        all_starters: bool,

        /// Model identifier (default: "veo-3.1-fast-generate-001", or "veo-3.1-generate-001").
        #[arg(short, long, default_value = "veo-3.1-fast-generate-001")]
        model: String,

        /// Directory containing reference AI keyframe images.
        #[arg(long, default_value = "assets/ai_keyframes")]
        keyframes_dir: PathBuf,

        /// Output directory for downloaded raw .mp4 videos.
        #[arg(long, default_value = "assets/videos")]
        videos_dir: PathBuf,

        /// Target output directory for final baked PNG strips and .flipbook.ron manifests.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,

        /// Target square frame dimension in pixels (default: 512).
        #[arg(short, long, default_value_t = 512)]
        size: u32,

        /// Playback framerate (default: 15).
        #[arg(short, long, default_value_t = 15)]
        fps: u32,

        /// Print generation prompts and configuration without executing API calls.
        #[arg(long)]
        dry_run: bool,
    },

    /// Bakes multiple action video clips (Idle, Walk, Attack) for a single unit into a combined atlas.
    #[command(name = "bake-unit-videos")]
    BakeUnitVideos {
        /// Archetype name (e.g. "marine", "zealot", "marauder").
        #[arg(short, long)]
        unit: String,

        /// Path to Idle animation MP4 video clip.
        #[arg(long)]
        idle: Option<PathBuf>,

        /// Path to Walk animation MP4 video clip.
        #[arg(long)]
        walk: Option<PathBuf>,

        /// Path to Attack animation MP4 video clip.
        #[arg(long)]
        attack: Option<PathBuf>,

        /// Target output directory for generated assets.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,

        /// Target square frame dimension in pixels.
        #[arg(short, long, default_value_t = 512)]
        size: u32,

        /// Playback framerate.
        #[arg(short, long, default_value_t = 15)]
        fps: u32,
    },

    /// Bakes a single video file into a flipbook atlas using FFmpeg.
    BakeVideo {
        /// Source video file path.
        #[arg(short, long)]
        input: PathBuf,

        /// Target output directory for generated assets.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,

        /// Target square frame dimension in pixels (width and height).
        #[arg(short, long, default_value_t = 256)]
        size: u32,

        /// Playback framerate in frames per second (FPS).
        #[arg(short, long, default_value_t = 24)]
        fps: u32,

        /// Unique atlas name prefix for output files.
        #[arg(short, long)]
        name: String,
    },

    /// Synthesizes a single procedural archetype atlas.
    Procedural {
        /// Archetype name (e.g. "marine", "command_dome", "zealot").
        #[arg(short, long)]
        archetype: String,

        /// Target output directory for generated assets.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,

        /// Frame dimension in pixels (defaults to 192 for units, 256 for domes).
        #[arg(short, long)]
        size: Option<u32>,

        /// Playback framerate (defaults to 15 FPS).
        #[arg(short, long)]
        fps: Option<u32>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateDefaults {
            output_dir,
            concept_video,
            size,
            fps,
        } => {
            if concept_video.exists() {
                println!(
                    "==> Baking starter unit video flipbooks from {:?} at {}x{} @ {} fps...",
                    concept_video, size, size, fps
                );
                let video_metas = asset_baker::bake_starter_units_from_real_video(
                    &concept_video,
                    &output_dir,
                    size,
                    fps,
                )?;
                println!(
                    "    Baked {} starter unit atlases successfully from real video.",
                    video_metas.len()
                );
            } else {
                eprintln!(
                    "--> Concept video {:?} not found, falling back to procedural atlases.",
                    concept_video
                );
                let procedural_metas = asset_baker::generate_all_procedural_atlases(&output_dir)?;
                println!(
                    "    Generated {} procedural flipbook atlases.",
                    procedural_metas.len()
                );
            }

            println!("==> Default asset baking complete.");
        }

        Commands::GenerateVeo {
            unit,
            all_starters,
            model,
            keyframes_dir,
            videos_dir,
            output_dir,
            size,
            fps,
            dry_run,
        } => {
            let units: Vec<String> = if all_starters {
                sim_core::UnitCatalog::default()
                    .archetypes()
                    .filter(|a| a.showcase_hero)
                    .map(|a| a.slug.clone())
                    .collect()
            } else if let Some(ref u) = unit {
                vec![u.clone()]
            } else {
                return Err(
                    "Either `--unit <name>` or `--all-starters` must be specified for Veo generation."
                        .into(),
                );
            };

            let client = match asset_baker::veo_client::VeoClient::new_auto() {
                Ok(c) => c.with_model(&model),
                Err(e) => {
                    eprintln!("Error resolving Veo API credentials: {e}");
                    return Err(Box::new(e));
                }
            };

            println!(
                "==> Google Veo Video Generation Pipeline (Model: {}, Mode: {})",
                client.model(),
                if client.is_vertex_ai() {
                    "Vertex AI Express"
                } else {
                    "Gemini API"
                }
            );

            for u in units {
                if dry_run {
                    println!("\n--- [DRY RUN] Plan for unit: {} ---", u);
                    for state in [AnimState::Idle, AnimState::Walk, AnimState::Attack] {
                        let prompt = asset_baker::build_action_prompt(&u, state);
                        let state_str = match state {
                            AnimState::Idle => "idle",
                            AnimState::Walk => "walk",
                            AnimState::Attack => "attack",
                            AnimState::Constructing => "constructing",
                            AnimState::Cast => "cast",
                            AnimState::Die => "die",
                        };
                        let keyframe_path = keyframes_dir.join(format!("{u}_{state_str}.jpg"));
                        let active_keyframe = keyframes_dir.join(format!("{u}_active.jpg"));
                        let idle_keyframe = keyframes_dir.join(format!("{u}_idle.jpg"));

                        let resolved_keyframe = if keyframe_path.exists() {
                            Some(keyframe_path)
                        } else if state == AnimState::Attack && active_keyframe.exists() {
                            Some(active_keyframe)
                        } else if idle_keyframe.exists() {
                            Some(idle_keyframe)
                        } else {
                            None
                        };
                        println!("Action: {:?}", state);
                        println!("Keyframe: {:?}", resolved_keyframe);
                        println!("Prompt: {}", prompt);
                    }
                    continue;
                }

                println!("\n==> Processing unit '{}' via Veo API...", u);
                let meta = client.generate_and_bake_unit(
                    &u,
                    &keyframes_dir,
                    &videos_dir,
                    &output_dir,
                    size,
                    fps,
                )?;
                println!(
                    "    Successfully baked '{}' ({} frames at {}x{} @ {} fps).",
                    meta.name, meta.total_frames, meta.frame_width, meta.frame_height, meta.fps
                );
            }
        }

        Commands::BakeUnitVideos {
            unit,
            idle,
            walk,
            attack,
            output_dir,
            size,
            fps,
        } => {
            let mut clips: Vec<(AnimState, PathBuf)> = Vec::new();
            if let Some(p) = idle {
                clips.push((AnimState::Idle, p));
            }
            if let Some(p) = walk {
                clips.push((AnimState::Walk, p));
            }
            if let Some(p) = attack {
                clips.push((AnimState::Attack, p));
            }

            if clips.is_empty() {
                return Err(
                    "At least one of `--idle`, `--walk`, or `--attack` must be provided.".into(),
                );
            }

            println!(
                "==> Baking {} action video clips for unit '{}' into {:?}...",
                clips.len(),
                unit,
                output_dir
            );
            let meta = asset_baker::bake_multi_action_videos_with_states(
                &unit,
                &clips,
                &output_dir,
                size,
                fps,
            )?;
            println!(
                "    Successfully generated multi-action atlas '{}' ({} frames at {}x{} @ {} fps).",
                meta.name, meta.total_frames, meta.frame_width, meta.frame_height, meta.fps
            );
        }

        Commands::BakeVideo {
            input,
            output_dir,
            size,
            fps,
            name,
        } => {
            println!(
                "==> Baking video {:?} into atlas '{}' at {}x{} @ {} fps...",
                input, name, size, size, fps
            );
            let meta = asset_baker::bake_custom_video(&input, &output_dir, &name, size, fps)?;
            println!(
                "    Successfully generated '{}' with {} frames.",
                meta.name, meta.total_frames
            );
        }

        Commands::Procedural {
            archetype,
            output_dir,
            size,
            fps,
        } => {
            println!("==> Synthesizing procedural archetype '{}'...", archetype);
            let output = asset_baker::generate_procedural_atlas(&archetype, size, fps)?;
            std::fs::create_dir_all(&output_dir)?;

            let png_path = output_dir.join(format!("{}.png", output.meta.name));
            let ron_path = output_dir.join(format!("{}.flipbook.ron", output.meta.name));

            output.image.save(&png_path)?;
            output.meta.save_to_ron(&ron_path)?;

            println!(
                "    Successfully generated '{}' ({} frames at {}x{} @ {} fps).",
                output.meta.name,
                output.meta.total_frames,
                output.meta.frame_width,
                output.meta.frame_height,
                output.meta.fps
            );
        }
    }

    Ok(())
}
