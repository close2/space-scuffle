//! Command-line interface for the Space Scuffle asset baker.
//!
//! # Responsibility
//! Parses the command line and runs one subcommand: generate authentic video-backed flipbook
//! atlases with Google Veo, transcode multi-action videos with FFmpeg, audit installed media,
//! bake terrain themes, or synthesize procedural atlases for use as test fixtures.
//!
//! # How it fits
//! Each subcommand is one `run_*` function over the `asset_baker` library. Every command that
//! needs the unit catalog loads it from the asset bundle at run time (`--assets <dir>`, else the
//! search in [`asset_baker::resolve_assets_root`]), so editing `units.ron` needs no rebuild.
//!
//! # Invariants
//! - The catalog is read once per run, from disk; nothing is compiled in.
//! - A failure is returned from `main` and ends the process with a nonzero status.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(
    clippy::doc_markdown,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting
)]

use clap::{Parser, Subcommand};
use sim_core::UnitCatalog;
use sim_types::enums::AnimState;
use std::error::Error;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "asset_baker")]
#[command(author = "Space Scuffle Systems Engineering Team")]
#[command(version = "0.1.0")]
#[command(about = "Space Scuffle video and procedural flipbook atlas baker")]
struct Cli {
    /// Root of the asset bundle whose `catalog/` is loaded (default: `SPACE_SCUFFLE_ASSETS`, else
    /// the nearest `assets` directory above the crate or the current directory).
    #[arg(long, global = true)]
    assets: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Audits exact installed media without baking or changing assets.
    AuditMedia {
        /// External catalog directory loaded at command invocation (default: `<assets>/catalog`).
        #[arg(long)]
        catalog_dir: Option<PathBuf>,
        /// Machine-readable JSON report destination (existing files are refused).
        #[arg(long)]
        report: PathBuf,
        /// Exit nonzero when provenance or required assets remain unapproved.
        #[arg(long)]
        strict: bool,
    },
    /// Retired unsafe roster-copy command; fails without reading or writing assets.
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
        /// Exact unit slug from the catalog. Near matches are refused.
        #[arg(short, long)]
        unit: Option<String>,

        /// Flag to generate every showcase hero in the catalog.
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

    /// Synthesizes a single procedural atlas as a test fixture, never as game media.
    Procedural {
        /// Exact archetype slug from the catalog. Near matches are refused.
        #[arg(short, long)]
        archetype: String,

        /// Output directory, which must be outside any runtime asset bundle.
        #[arg(short, long)]
        output_dir: PathBuf,

        /// Frame dimension in pixels (defaults to 192 for units, 256 for domes).
        #[arg(short, long)]
        size: Option<u32>,

        /// Playback framerate (defaults to 15 FPS).
        #[arg(short, long)]
        fps: Option<u32>,
    },

    /// Bakes a terrain visual theme into tileable albedo, normal, and ORM textures.
    #[command(name = "bake-terrain-theme")]
    BakeTerrainTheme {
        /// Unique theme slug to bake (e.g. "orbital_foundry").
        #[arg(short, long)]
        theme: String,

        /// Path to the terrain theme catalog RON file (default:
        /// `<assets>/catalog/terrain_themes.ron`).
        #[arg(short, long)]
        catalog: Option<PathBuf>,
    },
}

/// Result of a subcommand; any error ends the process with a nonzero status.
type RunResult = Result<(), Box<dyn Error>>;

fn main() -> RunResult {
    let cli = Cli::parse();
    let assets = cli.assets.as_deref();
    match cli.command {
        Commands::AuditMedia {
            catalog_dir,
            report,
            strict,
        } => run_audit_media(assets, catalog_dir, &report, strict),
        Commands::GenerateDefaults { .. } => {
            Err(asset_baker::BakerError::ExplicitSourcesRequired.into())
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
            let catalog = load_unit_catalog(assets)?;
            let units = veo_units(&catalog, unit, all_starters)?;
            let dirs = asset_baker::UnitMediaDirs {
                keyframes: &keyframes_dir,
                videos: &videos_dir,
                output: &output_dir,
            };
            run_generate_veo(&catalog, &units, &model, &dirs, size, fps, dry_run)
        }
        Commands::BakeUnitVideos {
            unit,
            idle,
            walk,
            attack,
            output_dir,
            size,
            fps,
        } => run_bake_unit_videos(&unit, [idle, walk, attack], &output_dir, size, fps),
        Commands::BakeVideo {
            input,
            output_dir,
            size,
            fps,
            name,
        } => run_bake_video(&input, &output_dir, size, fps, &name),
        Commands::Procedural {
            archetype,
            output_dir,
            size,
            fps,
        } => run_procedural(assets, &archetype, &output_dir, size, fps),
        Commands::BakeTerrainTheme { theme, catalog } => {
            run_bake_terrain_theme(assets, &theme, catalog)
        }
    }
}

/// Loads the unit catalog from the asset bundle chosen by `--assets` or the search.
fn load_unit_catalog(assets: Option<&Path>) -> Result<UnitCatalog, asset_baker::BakerError> {
    let root = asset_baker::resolve_assets_root(assets)?;
    Ok(asset_baker::load_catalogs(&root)?.units)
}

/// `audit-media`: reports the installed media of every catalog unit.
fn run_audit_media(
    assets: Option<&Path>,
    catalog_dir: Option<PathBuf>,
    report: &Path,
    strict: bool,
) -> RunResult {
    let root = asset_baker::resolve_assets_root(assets)?;
    let catalog_dir = catalog_dir.unwrap_or_else(|| root.join("catalog"));
    let catalogs = sim_core::catalog::RuntimeCatalogs::load_from_directory(catalog_dir)?;
    let audit = asset_baker::media_audit::audit_media(&root, &catalogs.units);
    audit.write_json(report)?;
    for entry in &audit.entries {
        println!("{}: {:?}", entry.slug, entry.status);
    }
    println!(
        "Report: {}. Structural media availability is not artistic approval.",
        report.display()
    );
    if strict {
        audit.require_verified()?;
    }
    Ok(())
}

/// The units `generate-veo` works on: every showcase hero, or the one named, each checked
/// against the catalog before credentials are read or anything is spent.
fn veo_units(
    catalog: &UnitCatalog,
    unit: Option<String>,
    all_starters: bool,
) -> Result<Vec<String>, Box<dyn Error>> {
    let units: Vec<String> = if all_starters {
        catalog
            .archetypes()
            .filter(|a| a.showcase_hero)
            .map(|a| a.slug.clone())
            .collect()
    } else if let Some(unit) = unit {
        vec![unit]
    } else {
        return Err(
            "Either `--unit <name>` or `--all-starters` must be specified for Veo generation."
                .into(),
        );
    };
    for unit in &units {
        asset_baker::validate_catalog_slug(catalog, unit)?;
    }
    Ok(units)
}

/// `generate-veo --dry-run`: prints each unit's prompts and keyframes without any API call.
fn print_veo_plan(catalog: &UnitCatalog, units: &[String], keyframes_dir: &Path) -> RunResult {
    for u in units {
        println!("\n--- [DRY RUN] Plan for unit: {} ---", u);
        for state in [AnimState::Idle, AnimState::Walk, AnimState::Attack] {
            let prompt = asset_baker::build_action_prompt(catalog, u, state)?;
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
    }
    Ok(())
}

/// `generate-veo`: generates and bakes each unit's clips with the Veo API, or with `dry_run`
/// prints the plan. Credentials are resolved first in both cases. `size` is the frame edge in
/// pixels and `fps` the playback rate.
fn run_generate_veo(
    catalog: &UnitCatalog,
    units: &[String],
    model: &str,
    dirs: &asset_baker::UnitMediaDirs<'_>,
    size: u32,
    fps: u32,
    dry_run: bool,
) -> RunResult {
    let client = match asset_baker::veo_client::VeoClient::new_auto() {
        Ok(c) => c.with_model(model),
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

    if dry_run {
        return print_veo_plan(catalog, units, dirs.keyframes);
    }
    for u in units {
        println!("\n==> Processing unit '{}' via Veo API...", u);
        let meta = client.generate_and_bake_unit(catalog, u, dirs, size, fps)?;
        println!(
            "    Successfully baked '{}' ({} frames at {}x{} @ {} fps).",
            meta.name, meta.total_frames, meta.frame_width, meta.frame_height, meta.fps
        );
    }
    Ok(())
}

/// `bake-unit-videos`: packs the given idle, walk and attack clips into one atlas.
fn run_bake_unit_videos(
    unit: &str,
    [idle, walk, attack]: [Option<PathBuf>; 3],
    output_dir: &Path,
    size: u32,
    fps: u32,
) -> RunResult {
    let clips: Vec<(AnimState, PathBuf)> = [
        (AnimState::Idle, idle),
        (AnimState::Walk, walk),
        (AnimState::Attack, attack),
    ]
    .into_iter()
    .filter_map(|(state, path)| path.map(|p| (state, p)))
    .collect();

    if clips.is_empty() {
        return Err("At least one of `--idle`, `--walk`, or `--attack` must be provided.".into());
    }

    println!(
        "==> Baking {} action video clips for unit '{}' into {:?}...",
        clips.len(),
        unit,
        output_dir
    );
    let meta =
        asset_baker::bake_multi_action_videos_with_states(unit, &clips, output_dir, size, fps)?;
    println!(
        "    Successfully generated multi-action atlas '{}' ({} frames at {}x{} @ {} fps).",
        meta.name, meta.total_frames, meta.frame_width, meta.frame_height, meta.fps
    );
    Ok(())
}

/// `bake-video`: bakes one video file into a flipbook atlas.
fn run_bake_video(input: &Path, output_dir: &Path, size: u32, fps: u32, name: &str) -> RunResult {
    println!(
        "==> Baking video {:?} into atlas '{}' at {}x{} @ {} fps...",
        input, name, size, size, fps
    );
    let meta = asset_baker::bake_custom_video(input, output_dir, name, size, fps)?;
    println!(
        "    Successfully generated '{}' with {} frames.",
        meta.name, meta.total_frames
    );
    Ok(())
}

/// `procedural`: synthesizes one archetype's test-fixture atlas from the run-time catalog.
fn run_procedural(
    assets: Option<&Path>,
    archetype: &str,
    output_dir: &Path,
    size: Option<u32>,
    fps: Option<u32>,
) -> RunResult {
    println!("==> Synthesizing procedural archetype '{}'...", archetype);
    // Checked first so that a refused path does not cost the render.
    asset_baker::ensure_outside_runtime_assets(output_dir)?;
    let catalog = load_unit_catalog(assets)?;
    let output = asset_baker::generate_procedural_atlas(&catalog, archetype, size, fps)?;
    asset_baker::write_procedural_atlas(&output, output_dir)?;

    println!(
        "    Successfully generated '{}' ({} frames at {}x{} @ {} fps).",
        output.meta.name,
        output.meta.total_frames,
        output.meta.frame_width,
        output.meta.frame_height,
        output.meta.fps
    );
    Ok(())
}

/// `bake-terrain-theme`: bakes one terrain theme's textures into the asset bundle.
fn run_bake_terrain_theme(
    assets: Option<&Path>,
    theme: &str,
    catalog: Option<PathBuf>,
) -> RunResult {
    let assets = asset_baker::resolve_assets_root(assets)?;
    let catalog = catalog.unwrap_or_else(|| assets.join("catalog/terrain_themes.ron"));
    println!(
        "==> Baking terrain theme '{theme}' from catalog {:?} (assets: {:?})...",
        catalog, assets
    );
    match asset_baker::bake_theme(&catalog, theme, &assets) {
        Ok(report) => {
            println!("{report}");
            println!("==> Terrain theme '{theme}' baked successfully.");
            Ok(())
        }
        Err(err) => {
            eprintln!("Error baking terrain theme '{theme}': {err}");
            Err(Box::new(err))
        }
    }
}
