//! Subcommands for the media of units' catalog modes (plan
//! `docs/plans/2026-11-09-effect-visualization.md` WP13; owner answer Q0075).
//!
//! # Responsibility
//! Parses and runs `mode-veo` (one paid clip of a mode atlas slice, or its dry run),
//! `mode-reject` (records a reviewer's rejection), `mode-withdraw-rejection` (withdraws one),
//! `reframe-mode-picture` (writes a reframed
//! mode's start picture), `install-mode-clip` (copies a reviewed
//! download into the bundle) and `bake-mode-videos` (packs a mode's installed clips into its
//! atlas with a provenance record) over [`asset_baker::mode_media`] and
//! [`asset_baker::paid_clip`].
//!
//! # How it fits
//! The workflow follows the unit clips' (`generate-veo`, `review`, `install-clip`,
//! `bake-unit-videos`): the prompt, pictures, price and total budget are checked before any
//! credential is read; every paid request is in the ledger; a download is reviewed before it is
//! installed; the bake records each clip's origin from the ledger. Contact sheets and loop
//! measurements come from the existing `keyframe-from-clip` and `loop-check` tools, which take
//! a clip file.
//!
//! # Invariants
//! - The catalog and the style file are read at run time; nothing names a unit or a mode.
//! - A slice with a download awaiting review is never generated again until the download is
//!   installed or rejected, so no clip is paid for twice by accident.
//! - Installing copies only bytes the ledger downloaded for exactly that slice and that no
//!   reviewer rejected.

use asset_baker::budget::{check_spend, BudgetConfig, SpendRequest, BUDGET_FILE};
use asset_baker::ffmpeg_importer::FrameSpec;
use asset_baker::framing::{DEFAULT_UNIT_FPS, DEFAULT_UNIT_FRAME_SIZE};
use asset_baker::ledger::{
    new_run_id, read_ledger_numbered, sha256_file, Ledger, LedgerEvent, LedgerHistory,
};
use asset_baker::mode_media::{
    build_mode_prompt, catalog_mode, check_mode_clip, plan_mode_pictures, required_mode_slices,
    ModeAtlasId, ModeMediaSpec, ModePictures,
};
use asset_baker::paid_clip::{run_paid_clip, ClipSubject, PaidClipJob};
use asset_baker::prompts::{
    compose_negative_prompt, ComposedPrompt, DrawingKind, KeyframeSource, MEDIA_STYLE_FILE,
};
use asset_baker::provenance::{
    bake_mode_with_provenance, veo_origin_of_mode_clip, ReframeRecord, SliceClip,
};
use asset_baker::veo_client::{
    VeoParameters, VeoRequestSettings, DEFAULT_VEO_MODEL, VEO_REQUEST_FILE,
};
use asset_baker::veo_plan::resolve_keyframe;
use asset_baker::{MediaStyle, SliceKey, VeoClient};
use clap::{Args, Subcommand};
use sim_core::UnitCatalog;
use sim_types::enums::AnimState;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

type RunResult = Result<(), Box<dyn Error>>;

/// One slice of one unit's mode atlas.
#[derive(Args, Debug)]
pub struct SliceArgs {
    /// Exact catalog slug.
    #[arg(short, long)]
    unit: String,
    /// The catalog mode (`mode_name`).
    #[arg(long)]
    mode: String,
    /// The slice: a state in the mode (`idle`, `attack`, ...) or a morph (`enter`, `leave`).
    #[arg(long, value_parser = SliceKey::parse)]
    slice: SliceKey,
}

/// The mode-media subcommands.
#[derive(Subcommand, Debug)]
pub enum ModeCommand {
    /// Generates one clip of a mode atlas slice with Veo (or resumes its outstanding
    /// operation). Paid: refused unless the prompt and pictures pass, the model is priced and
    /// the run fits the total budget. `--dry-run` prints the request and reads no credential.
    #[command(name = "mode-veo")]
    Veo {
        #[command(flatten)]
        slice: SliceArgs,
        /// Veo model id; it must have a price in `media/veo_budget.ron`.
        #[arg(long, default_value = DEFAULT_VEO_MODEL)]
        model: String,
        /// Directory of keyframes: the unit's Idle picture and the mode's picture.
        #[arg(long, default_value = "assets/ai_keyframes")]
        keyframes_dir: PathBuf,
        /// The most requests this run may start (1: one command generates one clip).
        #[arg(long)]
        max_generations: Option<u32>,
        /// The media ledger.
        #[arg(long)]
        ledger: PathBuf,
        /// Directory the clips are written to (e.g. `~/scratch/veo/mode-clips`).
        #[arg(long)]
        out_dir: PathBuf,
        /// Sample seed sent as `parameters.seed` (Vertex AI only); the ledger records it.
        #[arg(long)]
        seed: Option<u32>,
        /// Print the request and check it, without reading credentials or spending.
        #[arg(long)]
        dry_run: bool,
        /// Directory whose `.env` or `tmp/api-key.txt` (or its parent's) holds the key.
        #[arg(long)]
        credentials_dir: Option<PathBuf>,
        /// curl executable (tests point it at a loopback shim).
        #[arg(long, hide = true)]
        curl_path: Option<PathBuf>,
    },
    /// Records a reviewer's rejection of a downloaded mode clip; the slice may then be
    /// generated again.
    #[command(name = "mode-reject")]
    Reject {
        #[command(flatten)]
        slice: SliceArgs,
        /// The rejected download.
        #[arg(long)]
        clip: PathBuf,
        /// Why it is rejected, against the written review criteria.
        #[arg(long)]
        reason: String,
        /// The media ledger.
        #[arg(long)]
        ledger: PathBuf,
    },
    /// Withdraws a reviewer's earlier rejection of a downloaded mode clip, for example when a
    /// clip was rejected to allow a regeneration and every retry was worse, so the clip ships
    /// after all. Costs nothing.
    #[command(name = "mode-withdraw-rejection")]
    WithdrawRejection {
        #[command(flatten)]
        slice: SliceArgs,
        /// The clip whose rejection is withdrawn (the download, or its installed copy).
        #[arg(long)]
        clip: PathBuf,
        /// Why the rejection is withdrawn.
        #[arg(long)]
        reason: String,
        /// The media ledger.
        #[arg(long)]
        ledger: PathBuf,
    },
    /// Installs a reviewed mode clip download as `<videos>/<slug>-<mode>_<slice>.mp4`.
    #[command(name = "install-mode-clip")]
    Install {
        #[command(flatten)]
        slice: SliceArgs,
        /// The downloaded clip.
        #[arg(long)]
        clip: PathBuf,
        /// The media ledger.
        #[arg(long)]
        ledger: PathBuf,
        /// Directory of installed clips.
        #[arg(long, default_value = "assets/videos")]
        videos_dir: PathBuf,
        /// Replace an installed clip.
        #[arg(long)]
        replace: bool,
    },
    /// Writes a reframed mode's start picture, `<slug>-<mode>_reframed_idle.png`: the unit's
    /// Idle picture shrunk by the mode's `reframe` with a margin of its own backdrop. Free: it
    /// reads no credential and sends nothing.
    #[command(name = "reframe-mode-picture")]
    Reframe {
        /// Exact catalog slug.
        #[arg(short, long)]
        unit: String,
        /// The catalog mode (`mode_name`).
        #[arg(long)]
        mode: String,
        /// Directory of keyframes: the unit's Idle picture, and where the picture is written.
        #[arg(long, default_value = "assets/ai_keyframes")]
        keyframes_dir: PathBuf,
        /// Replace an existing reframed picture (the clips made from it then no longer chain).
        #[arg(long)]
        replace: bool,
    },
    /// Bakes a mode's installed clips into its atlas `<slug>-<mode>` with a provenance record.
    /// Every slice the mode needs must be installed.
    #[command(name = "bake-mode-videos")]
    Bake {
        /// Exact catalog slug.
        #[arg(short, long)]
        unit: String,
        /// The catalog mode (`mode_name`).
        #[arg(long)]
        mode: String,
        /// Directory of installed clips.
        #[arg(long, default_value = "assets/videos")]
        videos_dir: PathBuf,
        /// Target directory of the atlas.
        #[arg(short, long, default_value = "assets/textures")]
        output_dir: PathBuf,
        /// The media ledger, which holds each clip's `mode_clip_downloaded` line.
        #[arg(long)]
        ledger: PathBuf,
        /// Directory of keyframes, where a reframed mode's start picture is recorded from.
        #[arg(long, default_value = "assets/ai_keyframes")]
        keyframes_dir: PathBuf,
        /// Square frame side in pixels.
        #[arg(short, long, default_value_t = DEFAULT_UNIT_FRAME_SIZE)]
        size: u32,
        /// Playback rate.
        #[arg(short, long, default_value_t = DEFAULT_UNIT_FPS)]
        fps: u32,
    },
}

/// Runs one mode-media subcommand.
///
/// # Errors
/// Any failure of the command; see each `run_*` function.
pub fn run(assets: Option<&Path>, command: ModeCommand) -> RunResult {
    match command {
        ModeCommand::Veo {
            slice,
            model,
            keyframes_dir,
            max_generations,
            ledger,
            out_dir,
            seed,
            dry_run,
            credentials_dir,
            curl_path,
        } => run_veo(
            assets,
            &slice,
            &VeoRun {
                model,
                keyframes_dir,
                max_generations,
                ledger,
                out_dir,
                seed,
                dry_run,
                credentials_dir,
                curl_path,
            },
        ),
        ModeCommand::Reject {
            slice,
            clip,
            reason,
            ledger,
        } => run_reject(assets, &slice, &clip, &reason, &ledger),
        ModeCommand::WithdrawRejection {
            slice,
            clip,
            reason,
            ledger,
        } => run_withdraw(assets, &slice, &clip, &reason, &ledger),
        ModeCommand::Install {
            slice,
            clip,
            ledger,
            videos_dir,
            replace,
        } => run_install(assets, &slice, &clip, &ledger, &videos_dir, replace),
        ModeCommand::Reframe {
            unit,
            mode,
            keyframes_dir,
            replace,
        } => run_reframe(
            assets,
            ModeAtlasId {
                slug: &unit,
                stance: &mode,
            },
            &keyframes_dir,
            replace,
        ),
        ModeCommand::Bake {
            unit,
            mode,
            videos_dir,
            output_dir,
            ledger,
            keyframes_dir,
            size,
            fps,
        } => run_bake(
            assets,
            ModeAtlasId {
                slug: &unit,
                stance: &mode,
            },
            &BakeDirs {
                videos_dir,
                output_dir,
                ledger,
                keyframes_dir,
            },
            (size, fps),
        ),
    }
}

/// The run arguments of `mode-veo`.
struct VeoRun {
    model: String,
    keyframes_dir: PathBuf,
    max_generations: Option<u32>,
    ledger: PathBuf,
    out_dir: PathBuf,
    seed: Option<u32>,
    dry_run: bool,
    credentials_dir: Option<PathBuf>,
    curl_path: Option<PathBuf>,
}

/// The catalog and style of the bundle, checked against each other.
fn load(assets: Option<&Path>) -> Result<(PathBuf, UnitCatalog, MediaStyle), Box<dyn Error>> {
    let root = asset_baker::resolve_assets_root(assets)?;
    let catalog = asset_baker::load_catalogs(&root)?.units;
    let style = MediaStyle::load_for_catalog(&root, &catalog)?;
    style
        .modes
        .check_framing(&style.framing, &catalog, &root.join(MEDIA_STYLE_FILE))?;
    Ok((root, catalog, style))
}

/// Checks that `slice` names a slice its mode atlas needs and that the style describes it.
fn check_slice(
    catalog: &UnitCatalog,
    style: &MediaStyle,
    root: &Path,
    slice: &SliceArgs,
) -> Result<(), Box<dyn Error>> {
    asset_baker::validate_catalog_slug(catalog, &slice.unit)?;
    let archetype = catalog
        .get_by_slug(&slice.unit)
        .ok_or_else(|| format!("'{}' is not a catalog identity", slice.unit))?;
    let (_, mode) = catalog_mode(archetype, &slice.mode, &root.join(MEDIA_STYLE_FILE))?;
    let required = required_mode_slices(archetype, mode);
    if !required.contains(&slice.slice) {
        return Err(format!(
            "{}-{} never shows a {} slice (it needs {})",
            slice.unit,
            slice.mode,
            slice.slice,
            required
                .iter()
                .map(|slot| slot.stem())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    let id = ModeAtlasId {
        slug: &slice.unit,
        stance: &slice.mode,
    };
    if style.modes.spec(id).is_none() {
        return Err(format!(
            "media_style.ron's `modes` section has no entry for {}",
            id.atlas_name()
        )
        .into());
    }
    Ok(())
}

/// The checked request of `mode-veo`: the clip's prompt, its pictures and the parameters it
/// sends, printed as the dry run shows them.
///
/// # Errors
/// A missing Idle picture or mode entry, a picture the slice needs that does not exist yet, an
/// unreadable request file, or any prompt, picture or negative-prompt issue.
fn prepare_clip(
    root: &Path,
    catalog: &UnitCatalog,
    style: &MediaStyle,
    slice: &SliceArgs,
    run: &VeoRun,
) -> Result<(ComposedPrompt, ModePictures, VeoParameters), Box<dyn Error>> {
    let id = ModeAtlasId {
        slug: &slice.unit,
        stance: &slice.mode,
    };
    let style_path = root.join(MEDIA_STYLE_FILE);
    let spec = style
        .modes
        .spec(id)
        .ok_or_else(|| format!("no `modes` entry for {}", id.atlas_name()))?;
    let prompt = build_mode_prompt(catalog, style, id, slice.slice, &style_path)?;
    let KeyframeSource::File(default_idle) =
        resolve_keyframe(&run.keyframes_dir, id.slug, AnimState::Idle)
    else {
        return Err(format!(
            "'{}' has no Idle picture in '{}'; mode clips start from it",
            id.slug,
            run.keyframes_dir.display()
        )
        .into());
    };
    let pictures = plan_mode_pictures(spec, id, slice.slice, &default_idle, &run.keyframes_dir)?;
    let archetype = catalog
        .get_by_slug(id.slug)
        .ok_or_else(|| format!("'{}' is not a catalog identity", id.slug))?;
    let mut parameters = VeoRequestSettings::load(&root.join(VEO_REQUEST_FILE))?.parameters();
    parameters.negative_prompt = compose_negative_prompt(
        parameters.negative_prompt.as_deref(),
        style
            .actions
            .kind(DrawingKind::of(&archetype.shape))
            .negative_terms
            .as_deref(),
    );
    parameters.seed = run.seed;
    let mut issues = check_mode_clip(style, &prompt, &pictures);
    if let Some(negative) = &parameters.negative_prompt {
        issues.extend(asset_baker::prompts::check_negative_prompt(style, negative));
    }
    println!(
        "==> {} {} ({} characters):\n    {}",
        id.atlas_name(),
        slice.slice,
        prompt.text.chars().count(),
        prompt.text
    );
    println!("    Start picture: {}", pictures.start.display());
    match &pictures.end {
        Some(end) => println!("    End picture (lastFrame): {}", end.display()),
        None => println!("    End picture: none"),
    }
    println!("    Parameters: {parameters}");
    if !issues.is_empty() {
        let lines: Vec<String> = issues
            .iter()
            .map(|issue| format!("  {} {}: {issue}", id.atlas_name(), slice.slice))
            .collect();
        return Err(format!("the clip cannot be paid for:\n{}", lines.join("\n")).into());
    }
    Ok((prompt, pictures, parameters))
}

/// `mode-veo`: prompt, pictures, parameters, price and budget checks, then one generation (or
/// the resumption of the slice's outstanding operation).
fn run_veo(assets: Option<&Path>, slice: &SliceArgs, run: &VeoRun) -> RunResult {
    let (root, catalog, style) = load(assets)?;
    check_slice(&catalog, &style, &root, slice)?;
    let id = ModeAtlasId {
        slug: &slice.unit,
        stance: &slice.mode,
    };
    let (prompt, pictures, parameters) = prepare_clip(&root, &catalog, &style, slice, run)?;
    let history = LedgerHistory::load(&run.ledger)?;
    if let Some(downloaded) = history
        .mode_clip(id.slug, id.stance, slice.slice)
        .and_then(|clip| clip.downloaded.as_ref())
    {
        return Err(format!(
            "{} {}: the download '{}' awaits review; install it or reject it with `mode-reject` first",
            id.atlas_name(),
            slice.slice,
            downloaded.path.display()
        )
        .into());
    }
    let budget = BudgetConfig::load(&root.join(BUDGET_FILE))?;
    let price_cents = budget.price_cents(&run.model)?;
    let spent_cents = budget.spent_cents(&history)?;
    check_spend(
        &budget,
        SpendRequest {
            needed: 1,
            max_generations: run.max_generations,
            spent_cents,
            price_cents,
        },
    )?;
    println!("Budget: {}", budget.spend_line(spent_cents));
    if run.dry_run {
        println!("Dry run: nothing was sent and no credential was read.");
        return Ok(());
    }
    let client = transport(run)?;
    let mut ledger = Ledger::open(&run.ledger, new_run_id())?;
    let job = PaidClipJob {
        subject: ClipSubject::Mode {
            slug: id.slug,
            stance: id.stance,
            slot: slice.slice,
        },
        prompt: &prompt,
        keyframe: Some(&pictures.start),
        end_frame: pictures.end.as_deref(),
        parameters: &parameters,
        out_dir: &run.out_dir,
    };
    let result = run_paid_clip(&client, &mut ledger, &history, &job)?;
    println!(
        "{} {}: {} (SHA-256 {}); review it before installing",
        if result.started {
            "Generated"
        } else {
            "Resumed"
        },
        result.operation,
        result.path.display(),
        result.sha256
    );
    let after = LedgerHistory::load(&run.ledger)?;
    println!(
        "Budget after the run: {}",
        budget.spend_line(budget.spent_cents(&after)?)
    );
    Ok(())
}

/// The Veo client with credentials from the environment or `--credentials-dir`.
fn transport(run: &VeoRun) -> Result<VeoClient, Box<dyn Error>> {
    let search_root = match &run.credentials_dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir()?,
    };
    let credentials =
        asset_baker::resolve_credentials_from(|name| std::env::var(name).ok(), &search_root)?;
    let client = VeoClient::with_credentials(credentials).with_model(&run.model);
    Ok(match &run.curl_path {
        Some(path) => client.with_curl_path(path),
        None => client,
    })
}

/// `mode-reject`: records the rejection of a download of the slice.
fn run_reject(
    assets: Option<&Path>,
    slice: &SliceArgs,
    clip: &Path,
    reason: &str,
    ledger_path: &Path,
) -> RunResult {
    let (root, catalog, style) = load(assets)?;
    check_slice(&catalog, &style, &root, slice)?;
    if reason.trim().is_empty() {
        return Err("a rejection needs a reason".into());
    }
    let sha256 = downloaded_sha256(slice, clip, ledger_path)?;
    let mut ledger = Ledger::open(ledger_path, new_run_id())?;
    ledger.append(LedgerEvent::ModeClipRejected {
        slug: slice.unit.clone(),
        stance: slice.mode.clone(),
        slot: slice.slice,
        sha256: sha256.clone(),
        reason: reason.to_string(),
    })?;
    println!(
        "Rejected {}-{} {} ({sha256}); the slice may be generated again.",
        slice.unit, slice.mode, slice.slice
    );
    Ok(())
}

/// `mode-withdraw-rejection`: records that the rejection of a download of the slice is
/// withdrawn.
fn run_withdraw(
    assets: Option<&Path>,
    slice: &SliceArgs,
    clip: &Path,
    reason: &str,
    ledger_path: &Path,
) -> RunResult {
    let (root, catalog, style) = load(assets)?;
    check_slice(&catalog, &style, &root, slice)?;
    if reason.trim().is_empty() {
        return Err("a withdrawal needs a reason".into());
    }
    let sha256 = sha256_file(clip)?;
    let history = LedgerHistory::load(ledger_path)?;
    let rejected = history
        .mode_clip(&slice.unit, &slice.mode, slice.slice)
        .is_some_and(|known| known.rejected_sha256.contains(&sha256));
    if !rejected {
        return Err(format!(
            "'{}' ({sha256}) has no standing rejection for {}-{} {} in the ledger",
            clip.display(),
            slice.unit,
            slice.mode,
            slice.slice
        )
        .into());
    }
    let mut ledger = Ledger::open(ledger_path, new_run_id())?;
    ledger.append(LedgerEvent::ModeClipRejectionWithdrawn {
        slug: slice.unit.clone(),
        stance: slice.mode.clone(),
        slot: slice.slice,
        sha256: sha256.clone(),
        reason: reason.to_string(),
    })?;
    println!(
        "Withdrew the rejection of {}-{} {} ({sha256}); it counts as accepted again.",
        slice.unit, slice.mode, slice.slice
    );
    Ok(())
}

/// The SHA-256 of `clip`, which the ledger must have downloaded for exactly `slice` and no
/// reviewer may have rejected.
fn downloaded_sha256(
    slice: &SliceArgs,
    clip: &Path,
    ledger_path: &Path,
) -> Result<String, Box<dyn Error>> {
    let sha256 = sha256_file(clip)?;
    let history = LedgerHistory::load(ledger_path)?;
    let known = history.mode_clip(&slice.unit, &slice.mode, slice.slice);
    let downloaded = known.is_some_and(|clip| clip.downloaded_sha256.contains(&sha256));
    let rejected = known.is_some_and(|clip| clip.rejected_sha256.contains(&sha256));
    if !downloaded {
        return Err(format!(
            "'{}' ({sha256}) was never downloaded for {}-{} {} according to the ledger",
            clip.display(),
            slice.unit,
            slice.mode,
            slice.slice
        )
        .into());
    }
    if rejected {
        return Err(format!("'{}' ({sha256}) was rejected in review", clip.display()).into());
    }
    Ok(sha256)
}

/// `install-mode-clip`: copies a reviewed download into the videos directory.
fn run_install(
    assets: Option<&Path>,
    slice: &SliceArgs,
    clip: &Path,
    ledger_path: &Path,
    videos_dir: &Path,
    replace: bool,
) -> RunResult {
    let (root, catalog, style) = load(assets)?;
    check_slice(&catalog, &style, &root, slice)?;
    downloaded_sha256(slice, clip, ledger_path)?;
    let id = ModeAtlasId {
        slug: &slice.unit,
        stance: &slice.mode,
    };
    let target = videos_dir.join(id.clip_file(slice.slice));
    if target.exists() && !replace {
        return Err(format!(
            "'{}' is installed already; pass --replace to replace it",
            target.display()
        )
        .into());
    }
    std::fs::create_dir_all(videos_dir)?;
    std::fs::copy(clip, &target)?;
    println!("Installed {}", target.display());
    Ok(())
}

/// `reframe-mode-picture`: writes the reframed Idle picture of a mode with a `reframe`.
fn run_reframe(
    assets: Option<&Path>,
    id: ModeAtlasId<'_>,
    keyframes_dir: &Path,
    replace: bool,
) -> RunResult {
    let (_root, catalog, style) = load(assets)?;
    asset_baker::validate_catalog_slug(&catalog, id.slug)?;
    let spec = style
        .modes
        .spec(id)
        .ok_or_else(|| format!("no `modes` entry for {}", id.atlas_name()))?;
    let reframe = spec
        .reframe
        .ok_or_else(|| format!("{} has no `reframe` in media_style.ron", id.atlas_name()))?;
    let KeyframeSource::File(default_idle) =
        resolve_keyframe(keyframes_dir, id.slug, AnimState::Idle)
    else {
        return Err(format!(
            "'{}' has no Idle picture in '{}'",
            id.slug,
            keyframes_dir.display()
        )
        .into());
    };
    let target = id.reframed_idle_picture(keyframes_dir);
    if target.exists() && !replace {
        return Err(format!(
            "'{}' exists; pass --replace to write it again",
            target.display()
        )
        .into());
    }
    let source = image::open(&default_idle)?.to_rgba8();
    let reframed = asset_baker::framing::reframe_picture(
        &source,
        reframe.scale_permille,
        reframe.picture_feather_permille,
    )?;
    image::DynamicImage::ImageRgba8(reframed.image)
        .to_rgb8()
        .save(&target)?;
    let [r, g, b] = reframed.margin_rgb;
    println!(
        "Wrote '{}': '{}' at {} per mille on a margin of ({r}, {g}, {b}), SHA-256 {}.",
        target.display(),
        default_idle.display(),
        reframe.scale_permille,
        sha256_file(&target)?
    );
    Ok(())
}

/// Where `bake-mode-videos` reads and writes.
struct BakeDirs {
    videos_dir: PathBuf,
    output_dir: PathBuf,
    ledger: PathBuf,
    keyframes_dir: PathBuf,
}

/// The record of a reframed mode's start picture for its provenance, or `None` for a mode
/// without a `reframe`.
fn reframe_record(
    spec: &ModeMediaSpec,
    id: ModeAtlasId<'_>,
    keyframes_dir: &Path,
) -> Result<Option<ReframeRecord>, Box<dyn Error>> {
    let Some(reframe) = spec.reframe else {
        return Ok(None);
    };
    let picture = id.reframed_idle_picture(keyframes_dir);
    if !picture.is_file() {
        return Err(format!(
            "{} is reframed, and its reframed Idle picture '{}' is missing",
            id.atlas_name(),
            picture.display()
        )
        .into());
    }
    Ok(Some(ReframeRecord {
        scale_permille: reframe.scale_permille,
        picture_feather_permille: reframe.picture_feather_permille,
        picture_sha256: sha256_file(&picture)?,
    }))
}

/// `bake-mode-videos`: packs every slice the mode needs from its installed clip.
fn run_bake(
    assets: Option<&Path>,
    id: ModeAtlasId<'_>,
    dirs: &BakeDirs,
    (size, fps): (u32, u32),
) -> RunResult {
    let (root, catalog, style) = load(assets)?;
    asset_baker::validate_catalog_slug(&catalog, id.slug)?;
    let archetype = catalog
        .get_by_slug(id.slug)
        .ok_or_else(|| format!("'{}' is not a catalog identity", id.slug))?;
    let (_, mode) = catalog_mode(archetype, id.stance, &root.join(MEDIA_STYLE_FILE))?;
    let spec = style
        .modes
        .spec(id)
        .ok_or_else(|| format!("no `modes` entry for {}", id.atlas_name()))?;
    let records = read_ledger_numbered(&dirs.ledger)?;
    let mut clips = Vec::new();
    for slot in required_mode_slices(archetype, mode) {
        let path = dirs.videos_dir.join(id.clip_file(slot));
        if !path.is_file() {
            return Err(format!(
                "{} needs its {slot} clip, and '{}' is not installed",
                id.atlas_name(),
                path.display()
            )
            .into());
        }
        let sha256 = sha256_file(&path)?;
        let origin = veo_origin_of_mode_clip(&records, id.slug, id.stance, slot, &sha256)
            .ok_or_else(|| {
                format!(
                    "{} {slot}: '{}' has no `mode_clip_downloaded` line in the ledger; its provenance is unknown",
                    id.atlas_name(),
                    path.display()
                )
            })?;
        clips.push(SliceClip { slot, path, origin });
    }
    let seams: BTreeMap<AnimState, _> = spec
        .loops
        .iter()
        .filter_map(|(state, loop_spec)| Some((*state, loop_spec.seam?)))
        .collect();
    let framing = spec.bake_framing(
        style.framing.framing_for(id.slug, &archetype.shape),
        &root.join(MEDIA_STYLE_FILE),
    )?;
    println!(
        "==> Baking {} clips of {} into {:?} ({framing})...",
        clips.len(),
        id.atlas_name(),
        dirs.output_dir
    );
    let record = bake_mode_with_provenance(
        &id.atlas_name(),
        &clips,
        &dirs.videos_dir,
        &dirs.output_dir,
        FrameSpec { size, fps, framing },
        &seams,
        reframe_record(spec, id, &dirs.keyframes_dir)?,
    )?;
    println!(
        "    Wrote '{}': {} states and {} morphs at {}x{} @ {} fps, with its provenance record.",
        record.name,
        record.states.len(),
        record.morphs.len(),
        record.bake.frame_size,
        record.bake.frame_size,
        record.bake.fps
    );
    Ok(())
}
