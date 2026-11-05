//! Command-line interface for the Space Scuffle asset baker.
//!
//! # Responsibility
//! Parses the command line and runs one subcommand: plan, validate, check and run paid Veo
//! generations; review, install and import their clips; transcode multi-action videos with
//! FFmpeg; audit installed media; bake terrain themes; or synthesize procedural atlases for
//! use as test fixtures.
//!
//! # How it fits
//! Each subcommand is one `run_*` function over the `asset_baker` library. Every command that
//! needs the unit catalog loads it from the asset bundle at run time (`--assets <dir>`, else the
//! search in [`asset_baker::resolve_assets_root`]), so editing `units.ron` needs no rebuild.
//!
//! The paid Veo workflow (plan `docs/plans/2026-11-04-phase-8-5-veo-media.md`) is:
//! `validate-prompts` and `generate-veo --dry-run` (free, no credential read), `veo-preflight`
//! (one free authenticated GET), `generate-veo --max-generations N` (downloads to
//! `--clips-dir`, every request in `--ledger`), `review` (contact sheets, `--reject`, and for
//! each looping state the numbers of `loop-check`, the measured loop continuity),
//! `install-clip`, then `bake-unit-videos` (which writes `<slug>.provenance.ron` beside the
//! atlas), then `audit-media --strict --ledger <ledger>`. `review` and `bake-unit-videos` fit
//! each clip into the square game frame as the `framing` section of `media_style.ron` says for
//! the identity's render shape, and `bake-unit-videos` repairs the loop seam of each state the
//! `loops` section gives a `seam` (plan `docs/plans/2026-11-04-veo-seamless-loops.md`, D-5);
//! `generate-veo` sends the parameters of `media/veo_request.ron`.
//!
//! # Invariants
//! - The catalog is read once per run, from disk; nothing is compiled in.
//! - `generate-veo` validates its plan, prompts and budget before it reads any credential.
//! - Exit status: 0 on success; 3 when Veo refused a request for quota (wait, do not retry);
//!   4 when a run stopped on a result that needs a person (Cloud Storage URI, filtered, new
//!   failure kind); 2 for a usage error; 1 for any other failure.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![allow(
    clippy::doc_markdown,
    clippy::uninlined_format_args,
    clippy::unnecessary_debug_formatting
)]

use asset_baker::budget::{check_spend, format_usd, BudgetConfig, SpendRequest, BUDGET_FILE};
use asset_baker::clip_review::{self, DEFAULT_FFMPEG_PATH};
use asset_baker::ffmpeg_importer::{frames_per_state, FrameSpec, DEFAULT_FFPROBE_PATH};
use asset_baker::framing::{BakeFraming, DEFAULT_UNIT_FPS, DEFAULT_UNIT_FRAME_SIZE};
use asset_baker::ledger::{self, Ledger, LedgerHistory};
use asset_baker::media_audit::MediaAuditStatus;
use asset_baker::prompts::{atlas_loops, check_pin, SeamRepair};
use asset_baker::provenance;
use asset_baker::veo_plan::{parse_state, published_states, resolve_keyframe, state_slug, VeoDirs};
use asset_baker::{BakerError, MediaStyle, RunContext, VeoClient, VeoRequestSettings};
use clap::{Args, Parser, Subcommand};
use sim_core::UnitCatalog;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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

/// Which identities and states a Veo command works on, and where their files are.
#[derive(Args, Debug)]
struct Selection {
    /// Exact catalog slug; repeat for several identities. Near matches are refused.
    #[arg(short, long = "unit")]
    units: Vec<String>,

    /// Every showcase hero in the catalog.
    #[arg(long)]
    all_starters: bool,

    /// Only these published states, comma-separated (e.g. `idle,constructing`). A state the
    /// identity does not publish is refused.
    #[arg(long, value_delimiter = ',', value_parser = parse_state)]
    states: Vec<AnimState>,

    /// Directory of authored keyframes (`<slug>_<state>.jpg`).
    #[arg(long, default_value = "assets/ai_keyframes")]
    keyframes_dir: PathBuf,

    /// Directory of installed clips (`<slug>_<state>.mp4`).
    #[arg(long, default_value = "assets/videos")]
    videos_dir: PathBuf,
}

/// Arguments of `generate-veo`.
#[derive(Args, Debug)]
struct GenerateArgs {
    #[command(flatten)]
    selection: Selection,

    /// Directory for downloads awaiting review, `<slug>/<state>/<operation>.mp4` (outside the
    /// repository, e.g. `~/scratch/veo/clips`).
    #[arg(long)]
    clips_dir: PathBuf,

    /// The append-only ledger of every request (e.g. `docs/evidence/veo/ledger.jsonl`).
    #[arg(long)]
    ledger: PathBuf,

    /// Model identifier.
    #[arg(short, long, default_value = asset_baker::DEFAULT_VEO_MODEL)]
    model: String,

    /// Most generations this run may start, retries included. Required for a paid run: the
    /// run's stated intent, charged in full against the total budget before it starts.
    #[arg(long)]
    max_generations: Option<u32>,

    /// Budget file: total budget and price per generation (default:
    /// `<assets>/media/veo_budget.ron`).
    #[arg(long)]
    budget_file: Option<PathBuf>,

    /// Allow clips whose earlier attempts produced no usable clip to be generated again.
    #[arg(long)]
    allow_regeneration: bool,

    /// Directory whose `.env` or `tmp/api-key.txt` (or its parent's) holds the key (default:
    /// the current directory). The key is read by this process only and never printed.
    #[arg(long)]
    credentials_dir: Option<PathBuf>,

    /// Print the plan and check prompts and budget, without reading credentials or spending.
    #[arg(long)]
    dry_run: bool,

    /// Sample seed sent as `parameters.seed` with every request of this run (0 to 4294967295;
    /// Vertex AI only). The same seed with an otherwise unchanged request guides the model to
    /// the same clip; without it the service picks one. The ledger records it.
    #[arg(long)]
    seed: Option<u32>,

    /// Record a clip fetched by hand: `--import <SLUG> <STATE> <FILE> --operation <NAME>`.
    #[arg(long, num_args = 3, value_names = ["SLUG", "STATE", "FILE"])]
    import: Option<Vec<String>>,

    /// The recorded operation that produced an imported clip.
    #[arg(long)]
    operation: Option<String>,

    /// curl executable (tests point it at a loopback shim).
    #[arg(long, hide = true)]
    curl_path: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Audits exact installed media without baking or changing assets.
    AuditMedia(AuditArgs),
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

    /// Generates the published actions' clips with Google Veo into the clips directory.
    #[command(name = "generate-veo")]
    GenerateVeo(Box<GenerateArgs>),

    /// Checks the prompts and keyframes of every clip a run would pay for. Free.
    #[command(name = "validate-prompts")]
    ValidatePrompts {
        #[command(flatten)]
        selection: Selection,
        /// Ledger to read, so that clips awaiting review or resumable are not checked.
        #[arg(long)]
        ledger: Option<PathBuf>,
        /// Directory of downloads (only used to find clips the ledger recorded).
        #[arg(long)]
        clips_dir: Option<PathBuf>,
    },

    /// One free, authenticated GET of the model resource; prints mode, status and visibility.
    #[command(name = "veo-preflight")]
    VeoPreflight {
        /// Model identifier.
        #[arg(short, long, default_value = asset_baker::DEFAULT_VEO_MODEL)]
        model: String,
        /// Directory whose `.env` or `tmp/api-key.txt` (or its parent's) holds the key.
        #[arg(long)]
        credentials_dir: Option<PathBuf>,
        /// curl executable (tests point it at a loopback shim).
        #[arg(long, hide = true)]
        curl_path: Option<PathBuf>,
    },

    /// Writes the frame of a clip at a given time as a PNG.
    #[command(name = "keyframe-from-clip")]
    KeyframeFromClip {
        /// Source clip.
        #[arg(long)]
        clip: PathBuf,
        /// Time of the frame, in seconds.
        #[arg(long, default_value_t = 1.0)]
        at: f64,
        /// Destination PNG.
        #[arg(long)]
        output: PathBuf,
    },

    /// Writes contact sheets of an identity's clips (full 16:9 frames, and the framed squares
    /// that ship as `<slug>_<state>_framed.png`), or records a rejection with `--reject`.
    Review {
        /// Exact catalog slug.
        #[arg(short, long)]
        unit: String,
        /// Directory of installed clips.
        #[arg(long, default_value = "assets/videos")]
        videos_dir: PathBuf,
        /// The ledger.
        #[arg(long)]
        ledger: PathBuf,
        /// Full-size sheets (e.g. `~/scratch/veo/review`).
        #[arg(long)]
        sheets_dir: Option<PathBuf>,
        /// Downscaled sheets for the committed evidence (e.g. `docs/evidence/veo/review`).
        #[arg(long)]
        evidence_dir: Option<PathBuf>,
        /// Reject the download of this state awaiting review.
        #[arg(long, value_parser = parse_state, requires = "reason")]
        reject: Option<AnimState>,
        /// Why it is rejected, against the written review criteria.
        #[arg(long)]
        reason: Option<String>,
    },

    /// Measures the loop continuity of a looping state's clip on the frames its atlas will
    /// hold, raw and after the seam repair (loops plan D-6). Exits 1 when the repaired frames
    /// fail. Free: reads only local files.
    #[command(name = "loop-check")]
    LoopCheck(LoopCheckArgs),

    /// Installs a reviewed download as `<videos>/<slug>_<state>.mp4`.
    #[command(name = "install-clip")]
    InstallClip {
        /// Exact catalog slug.
        #[arg(short, long)]
        unit: String,
        /// Animation state.
        #[arg(long, value_parser = parse_state)]
        state: AnimState,
        /// The downloaded clip.
        #[arg(long)]
        clip: PathBuf,
        /// The ledger that recorded the download.
        #[arg(long)]
        ledger: PathBuf,
        /// Directory of installed clips.
        #[arg(long, default_value = "assets/videos")]
        videos_dir: PathBuf,
        /// Replace an installed clip.
        #[arg(long)]
        replace: bool,
    },

    /// Bakes an identity's action clips into one atlas; only states it publishes are accepted.
    /// Looping states get the seam repair `media_style.ron`'s `loops` section gives them.
    #[command(name = "bake-unit-videos")]
    BakeUnitVideos {
        /// Exact catalog slug.
        #[arg(short, long)]
        unit: String,

        /// Idle clip.
        #[arg(long)]
        idle: Option<PathBuf>,

        /// Walk clip.
        #[arg(long)]
        walk: Option<PathBuf>,

        /// Attack clip.
        #[arg(long)]
        attack: Option<PathBuf>,

        /// Constructing clip.
        #[arg(long)]
        constructing: Option<PathBuf>,

        /// Cast clip.
        #[arg(long)]
        cast: Option<PathBuf>,

        /// Target output directory for generated assets.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,

        /// Directory of installed clips; every clip must be a file directly inside it, because
        /// the provenance record names clips by file name.
        #[arg(long, default_value = "assets/videos")]
        videos_dir: PathBuf,

        /// Ledger in which to find the `downloaded` line of each clip this pipeline generated.
        #[arg(long)]
        ledger: Option<PathBuf>,

        /// `STATE=COMMIT`: the state's clip was committed by the original Veo work in COMMIT and
        /// has no ledger record (provenance origin `LegacyVeo`). Repeat per state.
        #[arg(long = "legacy-veo", value_name = "STATE=COMMIT")]
        legacy_veo: Vec<String>,

        /// Target square frame dimension in pixels.
        #[arg(short, long, default_value_t = DEFAULT_UNIT_FRAME_SIZE)]
        size: u32,

        /// Playback framerate.
        #[arg(short, long, default_value_t = DEFAULT_UNIT_FPS)]
        fps: u32,
    },

    /// Bakes a single video file into a flipbook atlas using FFmpeg (centre crop).
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

/// Exit status when Veo refused a request for quota.
const EXIT_QUOTA: u8 = 3;

/// Exit status when a run stopped on a result that needs a person.
const EXIT_STOPPED: u8 = 4;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            match error.downcast_ref::<BakerError>() {
                Some(BakerError::VeoQuotaExhausted { .. }) => ExitCode::from(EXIT_QUOTA),
                Some(BakerError::VeoRunStopped { .. }) => ExitCode::from(EXIT_STOPPED),
                _ => ExitCode::FAILURE,
            }
        }
    }
}

fn run(cli: Cli) -> RunResult {
    let assets = cli.assets.as_deref();
    match cli.command {
        Commands::AuditMedia(args) => run_audit_media(assets, args),
        Commands::GenerateDefaults { .. } => Err(BakerError::ExplicitSourcesRequired.into()),
        Commands::GenerateVeo(args) => run_generate_veo(assets, &args),
        Commands::ValidatePrompts {
            selection,
            ledger,
            clips_dir,
        } => run_validate_prompts(assets, &selection, ledger.as_deref(), clips_dir),
        Commands::VeoPreflight {
            model,
            credentials_dir,
            curl_path,
        } => run_veo_preflight(&model, credentials_dir.as_deref(), curl_path.as_deref()),
        Commands::LoopCheck(args) => run_loop_check(assets, &args),
        Commands::KeyframeFromClip { clip, at, output } => {
            run_keyframe_from_clip(&clip, at, &output)
        }
        Commands::Review {
            unit,
            videos_dir,
            ledger,
            sheets_dir,
            evidence_dir,
            reject,
            reason,
        } => {
            let target = ReviewTarget {
                unit: &unit,
                videos_dir,
                ledger: &ledger,
            };
            match (reject, reason) {
                (Some(state), Some(reason)) => run_reject(assets, &target, state, &reason),
                _ => run_review(assets, &target, sheets_dir, evidence_dir),
            }
        }
        Commands::InstallClip {
            unit,
            state,
            clip,
            ledger,
            videos_dir,
            replace,
        } => run_install_clip(assets, (&unit, state), &clip, &ledger, videos_dir, replace),
        Commands::BakeUnitVideos {
            unit,
            idle,
            walk,
            attack,
            constructing,
            cast,
            output_dir,
            videos_dir,
            ledger,
            legacy_veo,
            size,
            fps,
        } => {
            let clips = [
                (AnimState::Idle, idle),
                (AnimState::Constructing, constructing),
                (AnimState::Walk, walk),
                (AnimState::Attack, attack),
                (AnimState::Cast, cast),
            ];
            let target = BakeTarget {
                output_dir,
                videos_dir,
                ledger,
                legacy_veo,
                size,
                fps,
            };
            run_bake_unit_videos(assets, &unit, clips, &target)
        }
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
fn load_unit_catalog(assets: Option<&Path>) -> Result<UnitCatalog, BakerError> {
    let root = asset_baker::resolve_assets_root(assets)?;
    Ok(asset_baker::load_catalogs(&root)?.units)
}

/// Arguments of `loop-check`.
#[derive(Args, Debug)]
struct LoopCheckArgs {
    /// Exact catalog slug.
    #[arg(short, long)]
    unit: String,
    /// The looping state (idle, walk or cast) whose bake budget and seam repair apply.
    #[arg(long, value_parser = parse_state)]
    state: AnimState,
    /// The clip to measure (a download awaiting review, or an installed clip).
    #[arg(long)]
    clip: PathBuf,
    /// The picture the clip was pinned to; default: the identity's pin when the loop policy
    /// pins the state's end and the picture passes the pin rules.
    #[arg(long, conflicts_with = "no_pin")]
    pin: Option<PathBuf>,
    /// The clip was not pinned (text-to-video, or started from another picture): no start
    /// fidelity is measured.
    #[arg(long)]
    no_pin: bool,
    /// Directory of authored keyframes, where the default pin is looked up (default:
    /// `<assets>/ai_keyframes`).
    #[arg(long)]
    keyframes_dir: Option<PathBuf>,
    /// Full-size sheets of the judged frames (`<slug>_<state>_repaired.png` and `_seam.png`).
    #[arg(long, requires = "evidence_dir")]
    sheets_dir: Option<PathBuf>,
    /// Downscaled copies of the sheets for the committed evidence.
    #[arg(long, requires = "sheets_dir")]
    evidence_dir: Option<PathBuf>,
    /// Square frame side in pixels, as `bake-unit-videos --size`.
    #[arg(short, long, default_value_t = DEFAULT_UNIT_FRAME_SIZE)]
    size: u32,
    /// Playback rate, as `bake-unit-videos --fps`.
    #[arg(short, long, default_value_t = DEFAULT_UNIT_FPS)]
    fps: u32,
}

/// Which picture a loop check compares the clip's frame 0 with.
enum PinChoice {
    /// The identity's pin, if the loop policy pins the state and the picture passes the pin
    /// rules; looked up in this keyframes directory.
    Policy(PathBuf),
    /// This picture.
    File(PathBuf),
    /// None: the clip was not pinned.
    Unpinned,
}

/// One looping state of one identity, measured by [`run_one_loop_check`].
struct LoopCheckTarget<'a> {
    catalog: &'a UnitCatalog,
    style: &'a MediaStyle,
    unit: &'a str,
    state: AnimState,
    clip: &'a Path,
    spec: FrameSpec,
}

/// Measures one state's clip with the identity's budget, seam and pin, and prints the report.
fn run_one_loop_check(
    target: &LoopCheckTarget<'_>,
    pin: &PinChoice,
) -> Result<clip_review::LoopCheck, Box<dyn Error>> {
    let (unit, state) = (target.unit, target.state);
    published_states(target.catalog, unit, Some(&[state]))?;
    if !atlas_loops(state) {
        return Err(BakerError::InvalidArgument {
            reason: format!(
                "{unit} {}: the state does not loop, so it has no loop to check",
                state_slug(state)
            ),
        }
        .into());
    }
    let states = published_states(target.catalog, unit, None)?;
    let pin = match pin {
        PinChoice::Unpinned => None,
        PinChoice::File(path) => Some(path.clone()),
        PinChoice::Policy(keyframes) => target
            .style
            .loops
            .pins_end(unit, state)
            .then(|| resolve_keyframe(keyframes, unit, state))
            .and_then(|source| source.file().map(Path::to_path_buf))
            .filter(|path| check_pin(path).is_none()),
    };
    let label = format!("{unit} {}", state_slug(state));
    let request = clip_review::LoopCheckRequest {
        clip: target.clip,
        spec: target.spec,
        budget: frames_per_state(target.spec.size, states.len()),
        seam: target
            .style
            .loops
            .spec_for(unit, state)
            .and_then(|spec| spec.seam),
        pin: pin.as_deref(),
        label: &label,
    };
    let check = clip_review::loop_check(Path::new(DEFAULT_FFMPEG_PATH), &request)?;
    println!(
        "loop-check {label}: {} ({} px {}, {} fps, budget {} frames, pin {})",
        target.clip.display(),
        target.spec.size,
        target.spec.framing,
        target.spec.fps,
        request.budget,
        pin.as_deref()
            .map_or_else(|| "none".to_string(), |path| path.display().to_string())
    );
    println!("{}", check.render());
    Ok(check)
}

/// `loop-check`: the D-6 measures of one clip; fails when the repaired frames fail.
fn run_loop_check(assets: Option<&Path>, args: &LoopCheckArgs) -> RunResult {
    let (catalog, style) = identity_style(assets, &args.unit)?;
    let framing = framing_of(&catalog, &style, &args.unit)?;
    let pin = match (&args.pin, args.no_pin, &args.keyframes_dir) {
        (Some(path), _, _) => PinChoice::File(path.clone()),
        (None, true, _) => PinChoice::Unpinned,
        (None, false, Some(dir)) => PinChoice::Policy(dir.clone()),
        (None, false, None) => {
            PinChoice::Policy(asset_baker::resolve_assets_root(assets)?.join("ai_keyframes"))
        }
    };
    let target = LoopCheckTarget {
        catalog: &catalog,
        style: &style,
        unit: &args.unit,
        state: args.state,
        clip: &args.clip,
        spec: FrameSpec {
            size: args.size,
            fps: args.fps,
            framing,
        },
    };
    let check = run_one_loop_check(&target, &pin)?;
    if let (Some(sheets), Some(evidence)) = (&args.sheets_dir, &args.evidence_dir) {
        let stem = format!("{}_{}", args.unit, state_slug(args.state));
        let (repaired, seam) = clip_review::loop_sheets(&check, sheets, evidence, &stem)?;
        println!("  sheets: {} and {}", repaired.display(), seam.display());
    }
    if check.passes() {
        Ok(())
    } else {
        Err(format!(
            "{} {} fails the loop-continuity criterion",
            args.unit,
            state_slug(args.state)
        )
        .into())
    }
}

/// Arguments of `audit-media`.
#[derive(Args, Debug)]
struct AuditArgs {
    /// External catalog directory loaded at command invocation (default: `<assets>/catalog`).
    #[arg(long)]
    catalog_dir: Option<PathBuf>,
    /// Machine-readable JSON report destination (existing files are refused).
    #[arg(long)]
    report: PathBuf,
    /// Exit nonzero unless every identity of the required set is `Verified`.
    #[arg(long)]
    strict: bool,
    /// Required set (default: `<assets>/media/required_set.ron`).
    #[arg(long)]
    required_set: Option<PathBuf>,
    /// The Veo ledger, needed to verify clips generated by this pipeline (e.g.
    /// `docs/evidence/veo/ledger.jsonl`). Without it such clips are reported unverified.
    #[arg(long)]
    ledger: Option<PathBuf>,
}

/// `audit-media`: reports the installed media of every catalog unit and, with `--strict`,
/// fails unless every identity of the required set is `Verified`.
fn run_audit_media(assets: Option<&Path>, args: AuditArgs) -> RunResult {
    let root = asset_baker::resolve_assets_root(assets)?;
    let catalog_dir = args.catalog_dir.unwrap_or_else(|| root.join("catalog"));
    let catalogs = sim_core::catalog::RuntimeCatalogs::load_from_directory(catalog_dir)?;
    let required_path = args
        .required_set
        .unwrap_or_else(|| root.join(provenance::REQUIRED_SET_FILE));
    let required = provenance::RequiredSet::load(&required_path, &catalogs.units)?;
    let ledger = match &args.ledger {
        Some(path) => Some(ledger::read_ledger_numbered(path)?),
        None => None,
    };
    let audit = asset_baker::media_audit::audit_media(
        &root,
        &catalogs.units,
        &asset_baker::media_audit::AuditInputs {
            required: &required,
            ledger: ledger.as_deref(),
        },
    );
    audit.write_json(&args.report)?;
    for entry in &audit.entries {
        let tier = entry
            .required_tier
            .map_or(String::new(), |tier| format!(" [{tier:?}]"));
        println!("{}{tier}", entry.summary());
    }
    let verified = audit
        .entries
        .iter()
        .filter(|e| e.required_tier.is_some() && e.status == MediaAuditStatus::Verified)
        .count();
    println!(
        "Required set {}: {verified} of {} verified. Report: {}. Verified provenance is not artistic approval.",
        required_path.display(),
        required.identities.len(),
        args.report.display()
    );
    if args.strict {
        audit.require_verified()?;
    }
    Ok(())
}

/// The identities a Veo command works on: every showcase hero, or those named, each checked
/// against the catalog before credentials are read or anything is spent.
fn veo_units(catalog: &UnitCatalog, selection: &Selection) -> Result<Vec<String>, Box<dyn Error>> {
    let units: Vec<String> = if selection.all_starters {
        catalog
            .archetypes()
            .filter(|a| a.showcase_hero)
            .map(|a| a.slug.clone())
            .collect()
    } else if selection.units.is_empty() {
        return Err("either `--unit <slug>` or `--all-starters` must be given".into());
    } else {
        selection.units.clone()
    };
    for unit in &units {
        asset_baker::validate_catalog_slug(catalog, unit)?;
    }
    Ok(units)
}

/// The catalog, media style and plan of a Veo command.
struct PlannedRun {
    style: MediaStyle,
    root: PathBuf,
    history: LedgerHistory,
    dirs: VeoDirs,
    plan: asset_baker::RunPlan,
}

/// Loads the data (including `media/veo_request.ron`) and the ledger and plans the run of
/// `selection`, whose requests carry `seed` when one is given.
fn plan_selection(
    assets: Option<&Path>,
    selection: &Selection,
    clips_dir: PathBuf,
    ledger: Option<&Path>,
    seed: Option<u32>,
) -> Result<PlannedRun, Box<dyn Error>> {
    let root = asset_baker::resolve_assets_root(assets)?;
    let catalog = asset_baker::load_catalogs(&root)?.units;
    let style = MediaStyle::load_for_catalog(&root, &catalog)?;
    let request = VeoRequestSettings::load(&root.join(asset_baker::VEO_REQUEST_FILE))?;
    let parameters = asset_baker::VeoParameters {
        seed,
        ..request.parameters()
    };
    let units = veo_units(&catalog, selection)?;
    let history = match ledger {
        Some(path) => LedgerHistory::load(path)?,
        None => LedgerHistory::default(),
    };
    let dirs = VeoDirs {
        keyframes: selection.keyframes_dir.clone(),
        videos: selection.videos_dir.clone(),
        clips: clips_dir,
    };
    let filter = (!selection.states.is_empty()).then_some(selection.states.as_slice());
    let plan = asset_baker::plan_run(
        &catalog,
        &style,
        &parameters,
        &units,
        filter,
        &dirs,
        &history,
    )?;
    Ok(PlannedRun {
        style,
        root,
        history,
        dirs,
        plan,
    })
}

/// `validate-prompts`: refuses while any clip a run would pay for has a prompt or negative
/// prompt problem. Every action text of every kind and override is checked when the media
/// style loads. Reads no credential.
fn run_validate_prompts(
    assets: Option<&Path>,
    selection: &Selection,
    ledger: Option<&Path>,
    clips_dir: Option<PathBuf>,
) -> RunResult {
    let clips_dir = clips_dir.unwrap_or_else(|| PathBuf::from("clips"));
    let planned = plan_selection(assets, selection, clips_dir, ledger, None)?;
    let problems = planned.plan.problems(&planned.style);
    if !problems.is_empty() {
        return Err(BakerError::PromptsInvalid { problems }.into());
    }
    println!(
        "Prompts valid for {} clip(s) to generate.",
        planned.plan.generations()
    );
    Ok(())
}

/// `generate-veo`: plans, checks and (unless a dry run) executes a paid run, or imports a clip.
fn run_generate_veo(assets: Option<&Path>, args: &GenerateArgs) -> RunResult {
    if let Some(import) = &args.import {
        return run_import(assets, args, import);
    }
    let planned = plan_selection(
        assets,
        &args.selection,
        args.clips_dir.clone(),
        Some(&args.ledger),
        args.seed,
    )?;
    let mut stdout = std::io::stdout().lock();
    println!("==> Veo plan (model {}):", args.model);
    planned.plan.print(&mut stdout)?;
    println!(
        "Request parameters ({} and --seed; each clip sends its own negativePrompt): {}",
        planned.root.join(asset_baker::VEO_REQUEST_FILE).display(),
        planned.plan.parameters
    );
    let regenerations = planned.plan.regenerations();
    if !args.dry_run && !regenerations.is_empty() && !args.allow_regeneration {
        return Err(BakerError::RegenerationNotAllowed {
            clips: regenerations,
        }
        .into());
    }
    let problems = planned.plan.problems(&planned.style);
    if !problems.is_empty() {
        return Err(BakerError::PromptsInvalid { problems }.into());
    }
    let budget_file = args
        .budget_file
        .clone()
        .unwrap_or_else(|| planned.root.join(BUDGET_FILE));
    let config = BudgetConfig::load(&budget_file)?;
    let spent_cents = config.spent_cents(&planned.history)?;
    let price_cents = config.price_cents(&args.model)?;
    let needed = planned.plan.generations();
    println!("Budget: {}.", config.spend_line(spent_cents));
    println!(
        "Planned cost: {needed} generation(s) at {} = {}.",
        format_usd(price_cents),
        format_usd(u64::from(needed).saturating_mul(price_cents))
    );
    if args.dry_run {
        return Ok(());
    }
    check_spend(
        &config,
        SpendRequest {
            needed,
            max_generations: args.max_generations,
            spent_cents,
            price_cents,
        },
    )?;
    let result = execute(&planned, args, needed, &mut stdout);
    let after = LedgerHistory::load(&args.ledger)
        .map_err(BakerError::from)
        .and_then(|history| config.spent_cents(&history).map_err(BakerError::from));
    match after {
        Ok(spent) => println!("Budget after the run: {}.", config.spend_line(spent)),
        Err(error) => eprintln!("cannot total the spend after the run: {error}"),
    }
    result
}

/// Resolves credentials, opens the ledger and runs the plan, sending each clip's parameters
/// ([`asset_baker::RunPlan::parameters_for`]) with each of at most `needed` generations.
fn execute(
    planned: &PlannedRun,
    args: &GenerateArgs,
    needed: u32,
    out: &mut dyn std::io::Write,
) -> RunResult {
    let client = veo_client(
        &args.model,
        args.credentials_dir.as_deref(),
        args.curl_path.as_deref(),
    )?
    .with_parameters(planned.plan.parameters.clone());
    println!(
        "==> Veo run: {needed} generation(s) at most, {} with model {}.",
        client.mode().label(),
        client.model()
    );
    let mut ledger = Ledger::open(&args.ledger, ledger::new_run_id())?;
    let summary = asset_baker::execute_plan(
        &planned.plan,
        RunContext {
            client: &client,
            ledger: &mut ledger,
            history: &planned.history,
            dirs: &planned.dirs,
            max_generations: args.max_generations.unwrap_or(needed),
            out,
        },
    )?;
    println!(
        "==> Done: {} started, {} downloaded. Review with `asset_baker review` before installing.",
        summary.started, summary.downloaded
    );
    Ok(())
}

/// Resolves the key from the environment or `credentials_dir` and builds a client.
fn veo_client(
    model: &str,
    credentials_dir: Option<&Path>,
    curl_path: Option<&Path>,
) -> Result<VeoClient, BakerError> {
    let search_root = match credentials_dir {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir().map_err(|source| BakerError::Io {
            path: PathBuf::from("."),
            source,
        })?,
    };
    let credentials =
        asset_baker::resolve_credentials_from(|name| std::env::var(name).ok(), &search_root)?;
    let client = VeoClient::with_credentials(credentials).with_model(model);
    Ok(match curl_path {
        Some(path) => client.with_curl_path(path),
        None => client,
    })
}

/// `generate-veo --import`: records a clip fetched by hand against its recorded operation.
fn run_import(assets: Option<&Path>, args: &GenerateArgs, import: &[String]) -> RunResult {
    let [slug, state, file] = import else {
        return Err("--import takes <SLUG> <STATE> <FILE>".into());
    };
    let operation = args
        .operation
        .as_deref()
        .ok_or(BakerError::ImportNeedsOperation)?;
    let state = parse_state(state)?;
    let catalog = load_unit_catalog(assets)?;
    asset_baker::validate_catalog_slug(&catalog, slug)?;
    published_states(&catalog, slug, Some(&[state]))?;
    let history = LedgerHistory::load(&args.ledger)?;
    let dirs = VeoDirs {
        keyframes: args.selection.keyframes_dir.clone(),
        videos: args.selection.videos_dir.clone(),
        clips: args.clips_dir.clone(),
    };
    let mut ledger = Ledger::open(&args.ledger, ledger::new_run_id())?;
    let target = clip_review::import_clip(
        Path::new(DEFAULT_FFPROBE_PATH),
        &history,
        &mut ledger,
        &dirs,
        (slug, state),
        Path::new(file),
        operation,
    )?;
    println!(
        "Imported {} for {slug} {}; review it before installing.",
        target.display(),
        state_slug(state)
    );
    Ok(())
}

/// `veo-preflight`: one free authenticated GET; prints the mode, status and visibility only.
fn run_veo_preflight(
    model: &str,
    credentials_dir: Option<&Path>,
    curl_path: Option<&Path>,
) -> RunResult {
    let client = veo_client(model, credentials_dir, curl_path)?;
    let report = client.preflight()?;
    println!("Mode: {}", report.mode.label());
    println!("Model: {}", report.model);
    println!("HTTP status: {}", report.http_status);
    if let Some(status) = &report.error_status {
        println!("Error status: {status}");
    }
    let visible = report.model_visible();
    println!("Model visible: {}", if visible { "yes" } else { "no" });
    if visible {
        Ok(())
    } else {
        Err(format!(
            "the key cannot read model '{model}' (HTTP {})",
            report.http_status
        )
        .into())
    }
}

/// `keyframe-from-clip`: the frame of a clip at `at` seconds.
fn run_keyframe_from_clip(clip: &Path, at: f64, output: &Path) -> RunResult {
    let at = std::time::Duration::try_from_secs_f64(at).map_err(|error| {
        BakerError::InvalidArgument {
            reason: format!("--at {at}: {error}"),
        }
    })?;
    clip_review::extract_keyframe(Path::new(DEFAULT_FFMPEG_PATH), clip, at, output)?;
    println!("Wrote {}", output.display());
    Ok(())
}

/// The identity and files a `review` works on.
struct ReviewTarget<'a> {
    unit: &'a str,
    videos_dir: PathBuf,
    ledger: &'a Path,
}

impl ReviewTarget<'_> {
    fn dirs(&self) -> VeoDirs {
        VeoDirs {
            keyframes: PathBuf::new(),
            videos: self.videos_dir.clone(),
            clips: PathBuf::new(),
        }
    }
}

/// `review`: contact sheets of each published state's clip awaiting review, else installed.
fn run_review(
    assets: Option<&Path>,
    target: &ReviewTarget<'_>,
    sheets_dir: Option<PathBuf>,
    evidence_dir: Option<PathBuf>,
) -> RunResult {
    let (Some(sheets_dir), Some(evidence_dir)) = (sheets_dir, evidence_dir) else {
        return Err(
            "review needs --sheets-dir and --evidence-dir (or --reject and --reason)".into(),
        );
    };
    let (catalog, style) = identity_style(assets, target.unit)?;
    let framing = framing_of(&catalog, &style, target.unit)?;
    let spec = FrameSpec {
        size: DEFAULT_UNIT_FRAME_SIZE,
        fps: DEFAULT_UNIT_FPS,
        framing,
    };
    let keyframes = asset_baker::resolve_assets_root(assets)?.join("ai_keyframes");
    let history = LedgerHistory::load(target.ledger)?;
    let dirs = target.dirs();
    let mut written = 0;
    for state in published_states(&catalog, target.unit, None)? {
        let Some(clip) = clip_review::review_target(&history, &dirs, target.unit, state) else {
            println!("{} {}: no clip to review", target.unit, state_slug(state));
            continue;
        };
        let name = format!("{}_{}.png", target.unit, state_slug(state));
        clip_review::contact_sheet(
            Path::new(DEFAULT_FFMPEG_PATH),
            Path::new(DEFAULT_FFPROBE_PATH),
            &clip,
            &sheets_dir.join(&name),
            &evidence_dir.join(&name),
        )?;
        let framed_name = format!("{}_{}_framed.png", target.unit, state_slug(state));
        let bar = clip_review::framed_sheet(
            Path::new(DEFAULT_FFMPEG_PATH),
            &clip,
            spec,
            &sheets_dir.join(&framed_name),
            &evidence_dir.join(&framed_name),
        )?;
        println!(
            "{} {}: {} -> {} and {} ({framing}{})",
            target.unit,
            state_slug(state),
            clip.display(),
            sheets_dir.join(&name).display(),
            sheets_dir.join(&framed_name).display(),
            bar_note(bar)
        );
        if atlas_loops(state) {
            let check_target = LoopCheckTarget {
                catalog: &catalog,
                style: &style,
                unit: target.unit,
                state,
                clip: &clip,
                spec,
            };
            let check = run_one_loop_check(&check_target, &PinChoice::Policy(keyframes.clone()))?;
            let stem = format!("{}_{}", target.unit, state_slug(state));
            let (repaired, seam) =
                clip_review::loop_sheets(&check, &sheets_dir, &evidence_dir, &stem)?;
            println!("  sheets: {} and {}", repaired.display(), seam.display());
        }
        written += 1;
    }
    if written == 0 {
        return Err(format!("{} has no clip to review", target.unit).into());
    }
    Ok(())
}

/// The catalog and the media style, after checking that `unit` is a catalog identity.
fn identity_style(
    assets: Option<&Path>,
    unit: &str,
) -> Result<(UnitCatalog, MediaStyle), Box<dyn Error>> {
    let root = asset_baker::resolve_assets_root(assets)?;
    let catalog = asset_baker::load_catalogs(&root)?.units;
    asset_baker::validate_catalog_slug(&catalog, unit)?;
    let style = MediaStyle::load_for_catalog(&root, &catalog)?;
    Ok((catalog, style))
}

/// The framing `style` gives `unit`'s render shape.
fn framing_of(
    catalog: &UnitCatalog,
    style: &MediaStyle,
    unit: &str,
) -> Result<BakeFraming, Box<dyn Error>> {
    let shape = catalog
        .get_by_slug(unit)
        .map(|archetype| archetype.shape)
        .ok_or_else(|| format!("'{unit}' is not a catalog identity"))?;
    Ok(style.framing.framing_for(unit, &shape))
}

/// The seam repair `style`'s loop policy gives each of `states` of `unit` (loops plan, D-1 and
/// D-5); states without one are absent.
fn seam_repairs(
    style: &MediaStyle,
    unit: &str,
    states: &[AnimState],
) -> BTreeMap<AnimState, SeamRepair> {
    states
        .iter()
        .filter_map(|state| {
            let seam = style.loops.spec_for(unit, *state)?.seam?;
            Some((*state, seam))
        })
        .collect()
}

/// `, bars #rrggbb` for a measured bar colour, else nothing.
fn bar_note(bar: Option<[u8; 3]>) -> String {
    bar.map_or(String::new(), |[r, g, b]| {
        format!(", bars #{r:02x}{g:02x}{b:02x}")
    })
}

/// `review --reject`: records the rejection of a download awaiting review.
fn run_reject(
    assets: Option<&Path>,
    target: &ReviewTarget<'_>,
    state: AnimState,
    reason: &str,
) -> RunResult {
    let catalog = load_unit_catalog(assets)?;
    published_states(&catalog, target.unit, Some(&[state]))?;
    let history = LedgerHistory::load(target.ledger)?;
    let mut ledger = Ledger::open(target.ledger, ledger::new_run_id())?;
    let sha256 = clip_review::reject_clip(&history, &mut ledger, target.unit, state, reason)?;
    println!(
        "Rejected {} {} ({sha256}); the next run may regenerate it with --allow-regeneration.",
        target.unit,
        state_slug(state)
    );
    Ok(())
}

/// `install-clip`: installs a reviewed download.
fn run_install_clip(
    assets: Option<&Path>,
    (unit, state): (&str, AnimState),
    clip: &Path,
    ledger: &Path,
    videos_dir: PathBuf,
    replace: bool,
) -> RunResult {
    let catalog = load_unit_catalog(assets)?;
    published_states(&catalog, unit, Some(&[state]))?;
    let history = LedgerHistory::load(ledger)?;
    let dirs = VeoDirs {
        keyframes: PathBuf::new(),
        videos: videos_dir,
        clips: PathBuf::new(),
    };
    let target = clip_review::install_clip(
        Path::new(DEFAULT_FFPROBE_PATH),
        &history,
        &dirs,
        unit,
        state,
        clip,
        replace,
    )?;
    println!("Installed {}", target.display());
    Ok(())
}

/// Where and how `bake-unit-videos` bakes, and how it learns each clip's origin.
struct BakeTarget {
    output_dir: PathBuf,
    videos_dir: PathBuf,
    ledger: Option<PathBuf>,
    legacy_veo: Vec<String>,
    size: u32,
    fps: u32,
}

/// `bake-unit-videos`: packs the given clips into one atlas with its provenance record,
/// refusing states the identity does not publish and naming the published states that are
/// missing.
fn run_bake_unit_videos(
    assets: Option<&Path>,
    unit: &str,
    clips: [(AnimState, Option<PathBuf>); 5],
    target: &BakeTarget,
) -> RunResult {
    let clips: Vec<(AnimState, PathBuf)> = clips
        .into_iter()
        .filter_map(|(state, path)| path.map(|p| (state, p)))
        .collect();
    if clips.is_empty() {
        return Err(
            "at least one clip (--idle, --walk, --attack, --constructing, --cast) is needed".into(),
        );
    }
    let (catalog, style) = identity_style(assets, unit)?;
    let framing = framing_of(&catalog, &style, unit)?;
    let given: Vec<AnimState> = clips.iter().map(|(state, _)| *state).collect();
    let seams = seam_repairs(&style, unit, &given);
    published_states(&catalog, unit, Some(&given))?;
    let missing: Vec<&str> = published_states(&catalog, unit, None)?
        .into_iter()
        .filter(|state| !given.contains(state))
        .map(state_slug)
        .collect();
    if !missing.is_empty() {
        eprintln!(
            "Warning: '{unit}' also publishes {missing:?}; the game shows \"No approved media\" until they are baked."
        );
    }
    let clips = clip_origins(unit, clips, target)?;
    println!(
        "==> Baking {} action video clips for unit '{}' into {:?} ({framing})...",
        clips.len(),
        unit,
        target.output_dir
    );
    let spec = FrameSpec {
        size: target.size,
        fps: target.fps,
        framing,
    };
    let record = provenance::bake_with_provenance_and_seams(
        unit,
        &clips,
        &target.videos_dir,
        &target.output_dir,
        spec,
        &seams,
    )?;
    println!(
        "    Successfully generated multi-action atlas '{}' ({} states at {}x{} @ {} fps) with its provenance record.",
        record.name,
        record.states.len(),
        record.bake.frame_size,
        record.bake.frame_size,
        record.bake.fps
    );
    for (state, entry) in &record.states {
        if let Some(seam) = entry.seam {
            println!(
                "    {}: seam repair, loop sampling, scale lock {} per mille, cross-fade {} frames",
                state_slug(*state),
                seam.scale_permille,
                seam.crossfade_frames
            );
        }
        if let Some(bar) = entry.bar_rgb {
            let [r, g, b] = bar;
            println!("    {}: bars #{r:02x}{g:02x}{b:02x}", state_slug(*state));
        }
    }
    Ok(())
}

/// Parses `--legacy-veo STATE=COMMIT` values.
fn legacy_origins(values: &[String]) -> Result<BTreeMap<AnimState, String>, Box<dyn Error>> {
    let mut legacy = BTreeMap::new();
    for entry in values {
        let (state, commit) = entry
            .split_once('=')
            .ok_or_else(|| format!("--legacy-veo '{entry}' is not STATE=COMMIT"))?;
        if legacy
            .insert(parse_state(state)?, commit.to_string())
            .is_some()
        {
            return Err(format!("--legacy-veo names {state} twice").into());
        }
    }
    Ok(legacy)
}

/// The origin of each clip: `--legacy-veo` for the states it names, otherwise the ledger's
/// `downloaded` line of the clip's bytes. A clip with neither is refused before baking.
fn clip_origins(
    unit: &str,
    clips: Vec<(AnimState, PathBuf)>,
    target: &BakeTarget,
) -> Result<Vec<provenance::BakeClip>, Box<dyn Error>> {
    let mut legacy = legacy_origins(&target.legacy_veo)?;
    let records = match &target.ledger {
        Some(path) => ledger::read_ledger_numbered(path)?,
        None => Vec::new(),
    };
    let mut baked = Vec::with_capacity(clips.len());
    for (state, path) in clips {
        let origin = if let Some(commit) = legacy.remove(&state) {
            provenance::ClipOrigin::LegacyVeo { commit }
        } else {
            let sha256 = ledger::sha256_file(&path)?;
            provenance::veo_origin_from_ledger(&records, unit, state, &sha256).ok_or_else(|| {
                format!(
                    "{unit} {}: '{}' has no `downloaded` line in the ledger and no --legacy-veo origin; its provenance is unknown",
                    state_slug(state),
                    path.display()
                )
            })?
        };
        baked.push(provenance::BakeClip {
            state,
            path,
            origin,
        });
    }
    if let Some(state) = legacy.keys().next() {
        return Err(format!(
            "--legacy-veo names {}, for which no clip was given",
            state_slug(*state)
        )
        .into());
    }
    Ok(baked)
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
