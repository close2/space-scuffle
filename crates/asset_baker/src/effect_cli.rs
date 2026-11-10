//! The effect-texture subcommands of the `asset_baker` binary (plan
//! `docs/plans/2026-11-09-effect-visualization.md` §6, WP5).
//!
//! # Responsibility
//! Parses and runs `effect-noise`, `effect-bake`, `effect-review`, `effect-prompt`,
//! `effect-image` and `effect-image-preflight` over [`asset_baker::effects`].
//!
//! # How it fits
//! `main.rs` flattens [`EffectCommand`] into its subcommands. The paid command, `effect-image`,
//! follows the Veo workflow's order: the prompt, the model's price and the total budget are
//! checked before any credential is read; every request is in the media ledger.
//!
//! # Invariants
//! - Nothing here reads `tmp/` or `.env` itself; credentials are resolved only by the shared
//!   resolver, only for the two network commands, and never printed.
//! - `effect-image-preflight` generates nothing and costs nothing.

use asset_baker::budget::{check_spend, BudgetConfig, SpendRequest, BUDGET_FILE};
use asset_baker::effects::bake::{bake_noise, bake_recipe, BakeInputs, EffectBakeRecipe};
use asset_baker::effects::image_client::{
    run_image_job, ImageClient, ImageJob, DEFAULT_IMAGE_LOCATION, DEFAULT_IMAGE_SIZE,
};
use asset_baker::effects::noise::NoiseRecipe;
use asset_baker::effects::prompt::{
    build_effect_prompt, on_white_edit_prompt, EffectElementRequest, EffectMedium, EffectView,
};
use asset_baker::effects::review::{default_swatches, review_set};
use asset_baker::effects::texture_set::write_set;
use asset_baker::ledger::{new_run_id, read_ledger_numbered, Ledger, LedgerHistory};
use asset_baker::{BakerError, MediaStyle, VeoClient};
use clap::{Args, Subcommand};
use std::error::Error;
use std::path::{Path, PathBuf};

/// Result of a subcommand.
type RunResult = Result<(), Box<dyn Error>>;

/// What an effect-element request names.
#[derive(Args, Debug)]
pub struct ElementArgs {
    /// Effect texture set (e.g. `fx_blast`).
    #[arg(long)]
    set: String,
    /// Element of the set (e.g. `smoke_puff`).
    #[arg(long)]
    element: String,
    /// What the element is, in plain words; the prompt class adds black, view and medium.
    #[arg(long)]
    description: String,
    /// `side-on` or `top-down`.
    #[arg(long, default_value = "side-on")]
    view: EffectView,
}

/// Where the image client connects.
#[derive(Args, Debug)]
pub struct ImageModelArgs {
    /// Image model id; it must have a price in `media/veo_budget.ron`.
    #[arg(long)]
    model: String,
    /// Vertex AI location: `us-central1` or `global`.
    #[arg(long, default_value = DEFAULT_IMAGE_LOCATION)]
    location: String,
    /// Directory whose `.env` or `tmp/api-key.txt` (or its parent's) holds the key.
    #[arg(long)]
    credentials_dir: Option<PathBuf>,
    /// curl executable (tests point it at a loopback shim).
    #[arg(long, hide = true)]
    curl_path: Option<PathBuf>,
}

/// The effect-texture subcommands.
#[derive(Subcommand, Debug)]
pub enum EffectCommand {
    /// Writes a set of authored, tileable noise and erosion sheets from a noise recipe. Free.
    #[command(name = "effect-noise")]
    Noise {
        /// The noise recipe (`NoiseRecipe` RON).
        #[arg(long)]
        recipe: PathBuf,
        /// Directory the set is written to (e.g. `assets/effects`).
        #[arg(long)]
        output_dir: PathBuf,
    },
    /// Keys, packs and mips a set from its bake recipe; every source needs a ledger line. Free.
    #[command(name = "effect-bake")]
    Bake {
        /// The bake recipe (`EffectBakeRecipe` RON).
        #[arg(long)]
        recipe: PathBuf,
        /// Directory holding every source file the recipe names.
        #[arg(long)]
        sources_dir: PathBuf,
        /// The media ledger that recorded every source.
        #[arg(long)]
        ledger: PathBuf,
        /// Directory the set is written to (e.g. `assets/effects`).
        #[arg(long)]
        output_dir: PathBuf,
        /// FFmpeg executable, for clip sources.
        #[arg(long, default_value = "/usr/bin/ffmpeg")]
        ffmpeg: PathBuf,
    },
    /// Writes the texture contact sheets of a set. Free.
    #[command(name = "effect-review")]
    Review {
        /// The set.
        #[arg(long)]
        set: String,
        /// Directory of the set (default: `<assets>/effects`).
        #[arg(long)]
        effects_dir: Option<PathBuf>,
        /// Directory the sheets are written to.
        #[arg(long, default_value = "docs/evidence/effects/review")]
        output_dir: PathBuf,
        /// A terrain swatch picture; give two, or none for the first two shipped terrain
        /// albedo textures.
        #[arg(long = "swatch")]
        swatches: Vec<PathBuf>,
    },
    /// Prints the composed prompt of an effect-element request after checking it. Free.
    #[command(name = "effect-prompt")]
    Prompt {
        #[command(flatten)]
        element: ElementArgs,
        /// `still` (image model) or `clip` (Veo).
        #[arg(long, default_value = "still")]
        medium: EffectMedium,
    },
    /// Generates an effect element on black with an image model, and with `--pair` its edit
    /// on white. Paid: refused unless the model is priced and the run fits the total budget.
    #[command(name = "effect-image")]
    Image {
        #[command(flatten)]
        element: ElementArgs,
        #[command(flatten)]
        model: ImageModelArgs,
        /// Also request the same element on white, for a difference matte.
        #[arg(long)]
        pair: bool,
        /// Image size: `1K`, `2K` or `4K`.
        #[arg(long, default_value = DEFAULT_IMAGE_SIZE)]
        image_size: String,
        /// The most requests this run may start.
        #[arg(long)]
        max_generations: Option<u32>,
        /// The media ledger.
        #[arg(long)]
        ledger: PathBuf,
        /// Directory the images are written to (e.g. `~/scratch/effects/images`).
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// One free `:countTokens` POST on the image model: proves the key may call it without
    /// generating an image.
    #[command(name = "effect-image-preflight")]
    ImagePreflight {
        #[command(flatten)]
        model: ImageModelArgs,
    },
}

/// Runs one effect subcommand.
pub fn run(assets: Option<&Path>, command: EffectCommand) -> RunResult {
    match command {
        EffectCommand::Noise { recipe, output_dir } => {
            let recipe = NoiseRecipe::load(&recipe)?;
            let (manifest, _) = write_set(&output_dir, bake_noise(&recipe)?)?;
            report_set(&output_dir, &manifest.set, manifest.layers);
            Ok(())
        }
        EffectCommand::Bake {
            recipe,
            sources_dir,
            ledger,
            output_dir,
            ffmpeg,
        } => {
            let recipe = EffectBakeRecipe::load(&recipe)?;
            let ledger = read_ledger_numbered(&ledger)?;
            let inputs = BakeInputs {
                sources_dir: &sources_dir,
                ledger: &ledger,
                ffmpeg: &ffmpeg,
            };
            let (manifest, _) = write_set(&output_dir, bake_recipe(&recipe, inputs)?)?;
            report_set(&output_dir, &manifest.set, manifest.layers);
            Ok(())
        }
        EffectCommand::Review {
            set,
            effects_dir,
            output_dir,
            swatches,
        } => run_review(assets, &set, effects_dir, &output_dir, &swatches),
        EffectCommand::Prompt { element, medium } => {
            let style = MediaStyle::load(&asset_baker::resolve_assets_root(assets)?)?;
            let prompt = build_effect_prompt(&style, &request(&element, medium))?;
            println!("{}", prompt.text);
            Ok(())
        }
        EffectCommand::Image {
            element,
            model,
            pair,
            image_size,
            max_generations,
            ledger,
            out_dir,
        } => {
            let paid = PaidImageRun {
                pair,
                image_size: &image_size,
                max_generations,
                ledger: &ledger,
                out_dir: &out_dir,
            };
            run_image(assets, &element, &model, &paid)
        }
        EffectCommand::ImagePreflight { model } => run_preflight(&model),
    }
}

/// Prints where a set was written.
fn report_set(dir: &Path, set: &str, layers: u32) {
    println!(
        "Wrote effect texture set '{set}' ({layers} layer(s)) to {}",
        dir.display()
    );
}

/// The request named by `element` for `medium`.
fn request(element: &ElementArgs, medium: EffectMedium) -> EffectElementRequest {
    EffectElementRequest {
        set: element.set.clone(),
        element: element.element.clone(),
        description: element.description.clone(),
        view: element.view,
        medium,
    }
}

/// `effect-review`.
fn run_review(
    assets: Option<&Path>,
    set: &str,
    effects_dir: Option<PathBuf>,
    output_dir: &Path,
    swatches: &[PathBuf],
) -> RunResult {
    let root = asset_baker::resolve_assets_root(assets)?;
    let effects_dir = effects_dir.unwrap_or_else(|| root.join("effects"));
    let swatches = match swatches {
        [] => default_swatches(&root)?,
        [first, second] => [first.clone(), second.clone()],
        _ => return Err("give --swatch twice, or not at all".into()),
    };
    let output = review_set(&effects_dir, set, &swatches, output_dir)?;
    println!("Contact sheet: {}", output.sheet.display());
    if let Some(frames) = &output.frames {
        println!("Frame strips: {}", frames.display());
    }
    println!("Legend: {}", output.legend.display());
    println!("Loop GIF: not written (no GIF encoder is a dependency)");
    Ok(())
}

/// The paid run's own options.
struct PaidImageRun<'a> {
    pair: bool,
    image_size: &'a str,
    max_generations: Option<u32>,
    ledger: &'a Path,
    out_dir: &'a Path,
}

/// `effect-image`: prompt, price and budget checks, then the requests.
fn run_image(
    assets: Option<&Path>,
    element: &ElementArgs,
    model: &ImageModelArgs,
    paid: &PaidImageRun<'_>,
) -> RunResult {
    let root = asset_baker::resolve_assets_root(assets)?;
    let style = MediaStyle::load(&root)?;
    let prompt = build_effect_prompt(&style, &request(element, EffectMedium::Still))?;
    let edit = if paid.pair {
        Some(on_white_edit_prompt(&style)?)
    } else {
        None
    };
    let budget = BudgetConfig::load(&root.join(BUDGET_FILE))?;
    let price_cents = budget.price_cents(&model.model)?;
    let spent_cents = budget.spent_cents(&LedgerHistory::load(paid.ledger)?)?;
    check_spend(
        &budget,
        SpendRequest {
            needed: if paid.pair { 2 } else { 1 },
            max_generations: paid.max_generations,
            spent_cents,
            price_cents,
        },
    )?;
    println!("Budget: {}", budget.spend_line(spent_cents));
    let client = ImageClient::new(transport(model)?, &model.location, paid.image_size)?;
    let mut ledger = Ledger::open(paid.ledger, new_run_id())?;
    let job = ImageJob {
        set: &element.set,
        element: &element.element,
        prompt: &prompt,
        edit_prompt: edit.as_ref(),
        out_dir: paid.out_dir,
    };
    for saved in run_image_job(&client, &mut ledger, &job)? {
        println!(
            "{:?}: {} (SHA-256 {})",
            saved.role,
            saved.path.display(),
            saved.sha256
        );
    }
    Ok(())
}

/// `effect-image-preflight`: prints the mode, model, URL and status only.
fn run_preflight(model: &ImageModelArgs) -> RunResult {
    let client = ImageClient::new(transport(model)?, &model.location, DEFAULT_IMAGE_SIZE)?;
    let report = client.preflight()?;
    println!("Mode: {}", report.mode.label());
    println!("Model: {}", report.model);
    println!("URL: {}", report.url);
    println!("HTTP status: {}", report.http_status);
    if let Some(status) = &report.error_status {
        println!("Error status: {status}");
    }
    let callable = report.callable();
    println!("Model callable: {}", if callable { "yes" } else { "no" });
    if callable {
        Ok(())
    } else {
        Err(format!(
            "the key cannot call model '{}' (HTTP {})",
            report.model, report.http_status
        )
        .into())
    }
}

/// The Veo transport for the image model, with credentials from the environment or
/// `--credentials-dir`.
fn transport(model: &ImageModelArgs) -> Result<VeoClient, BakerError> {
    let search_root = match &model.credentials_dir {
        Some(dir) => dir.clone(),
        None => std::env::current_dir().map_err(|source| BakerError::Io {
            path: PathBuf::from("."),
            source,
        })?,
    };
    let credentials =
        asset_baker::resolve_credentials_from(|name| std::env::var(name).ok(), &search_root)?;
    let client = VeoClient::with_credentials(credentials).with_model(&model.model);
    Ok(match &model.curl_path {
        Some(path) => client.with_curl_path(path),
        None => client,
    })
}
