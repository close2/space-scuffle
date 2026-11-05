//! The showcase video: a storyboarded tour of a real battle, rendered by the production client.
//!
//! # Responsibility
//! This module owns the `showcase` tool (plan `docs/plans/2026-11-05-ci-showcase-video.md`):
//! - [`storyboard`]: the schema and validation of `assets/captures/showcase.ron`;
//! - [`cards`]: title and status cards generated from `DESIGN.md` and the plan index;
//! - [`pose`]: camera moves and the deterministic follow spring;
//! - [`director`], [`overlay`], [`recorder`] and [`take`]: one take rendered by the production
//!   client at a fixed 40 ms step into a lossless mezzanine and its manifest;
//! - [`encode`], [`site`] and [`render`]: the MP4, the WebP teaser, the poster, the player page
//!   and the provenance, and the `render` command CI and developers run.
//!
//! # How it fits
//! The tool builds the shipped client through `client::build_graphical_app`, like the capture and
//! frame-budget tools, and attaches itself through ordinary plugins and schedule sets. It never
//! changes the simulation: it issues no `PlayerCommand` and edits no entity (charter §3.2).
//!
//! # Invariants
//! - The video is engine output end to end; ffmpeg only cuts, scales and encodes (charter §1.2).
//! - Every failure is a typed [`ShowcaseError`] and a non-zero exit; nothing is published from a
//!   failed run.

pub mod cards;
pub mod director;
pub mod encode;
pub mod overlay;
pub mod pose;
pub mod recorder;
pub mod render;
pub mod site;
pub mod storyboard;
pub mod take;

use std::path::{Path, PathBuf};

/// The storyboard the tool renders by default, relative to the assets root.
pub const DEFAULT_STORYBOARD: &str = "captures/showcase.ron";

/// Failure of any showcase step.
#[derive(Debug, thiserror::Error)]
pub enum ShowcaseError {
    /// A file or directory could not be read or written.
    #[error("showcase I/O at {path}: {source}")]
    Io {
        /// The path involved.
        path: PathBuf,
        /// The filesystem error.
        #[source]
        source: std::io::Error,
    },
    /// A storyboard or manifest is not valid RON for its schema.
    #[error("cannot parse {path}: {source}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// The parser's diagnostic.
        #[source]
        source: ron::error::SpannedError,
    },
    /// The storyboard breaks a rule of plan decision D-4.
    #[error("invalid storyboard {path}: {fault}")]
    InvalidStoryboard {
        /// The storyboard.
        path: PathBuf,
        /// The first fault found.
        fault: storyboard::StoryboardFault,
    },
    /// A card could not be generated from the repository's documents.
    #[error("cannot build the showcase cards from {source_file}: {reason}")]
    Cards {
        /// The document that lacks a section, table or row.
        source_file: PathBuf,
        /// What is missing or malformed.
        reason: String,
    },
    /// The unit catalog could not be read.
    #[error(transparent)]
    Catalog(#[from] sim_core::catalog::CatalogLoadError),
    /// The assets root could not be resolved.
    #[error(transparent)]
    Assets(#[from] client::AssetsDirError),
    /// The production client could not start.
    #[error(transparent)]
    Startup(#[from] client::ClientStartupError),
    /// A required external program is not on `PATH`.
    #[error("`{0}` is required on PATH and could not be run")]
    MissingTool(&'static str),
    /// The storyboard has no take of that name, or the take's mode is reserved.
    #[error("the storyboard has no renderable take `{0}`")]
    UnknownTake(String),
    /// The mezzanine encoder failed.
    #[error(transparent)]
    Mezzanine(#[from] recorder::MezzanineError),
    /// An ffmpeg step of `assemble` failed, or its cut is inconsistent.
    #[error("assembling the {step}: {reason}")]
    Encode {
        /// The output being made.
        step: String,
        /// What went wrong.
        reason: String,
    },
    /// An output stays above its size budget at every rung of its ladder.
    #[error("{output} exceeds {budget} bytes at every step: {attempts:?}")]
    OverBudget {
        /// The output file.
        output: String,
        /// The budget, bytes.
        budget: u64,
        /// Every attempt, in order.
        attempts: Vec<String>,
    },
    /// `showcase render` could not launch or finish a take.
    #[error("render: {0}")]
    Render(String),
    /// A take failed while rendering; its manifest says why.
    #[error("take failed: {0}")]
    Take(String),
}

/// A storyboard that passed validation, with the cards built from the same repository.
#[derive(Debug, Clone)]
pub struct ValidatedStoryboard {
    /// The storyboard file.
    pub path: PathBuf,
    /// Its contents.
    pub storyboard: storyboard::Storyboard,
    /// The cards from `DESIGN.md` and the plan index.
    pub cards: cards::RepositoryCards,
}

impl ValidatedStoryboard {
    /// Loads `storyboard` (relative to `assets`), builds the cards from `repository`, reads the unit
    /// catalog from `assets/catalog`, and validates the storyboard against all three.
    ///
    /// # Errors
    /// Any read, parse, card or catalog error, or [`ShowcaseError::InvalidStoryboard`].
    pub fn load(
        assets: &Path,
        repository: &Path,
        storyboard: &Path,
    ) -> Result<Self, ShowcaseError> {
        let path = assets.join(storyboard);
        let parsed = storyboard::Storyboard::load(&path)?;
        let cards = cards::RepositoryCards::load(repository)?;
        let catalogs =
            sim_core::catalog::RuntimeCatalogs::load_from_directory(assets.join("catalog"))?;
        let textures = assets.join("textures");
        let context = storyboard::ValidationContext {
            subsystems: &cards.subsystems,
            catalog: &catalogs.units,
            textures: &textures,
        };
        parsed
            .validate(&context)
            .map_err(|fault| ShowcaseError::InvalidStoryboard {
                path: path.clone(),
                fault,
            })?;
        Ok(Self {
            path,
            storyboard: parsed,
            cards,
        })
    }
}
