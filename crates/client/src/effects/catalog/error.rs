//! Typed failures of loading the effect vocabulary and the units' presentation blocks.
//!
//! # Responsibility
//! Defines [`EffectCatalogError`], the one error type of `client::effects::catalog`. Every
//! variant names the file it came from and, where it applies, the effect and layer, so an
//! author can find the line to fix (plan `2026-11-09-effect-visualization` §5.2).
//!
//! # How it fits
//! The loader ([`super::EffectCatalog::load`]) and the unit tables
//! ([`super::UnitEffectTables::build`]) return it; the plugin logs it and exits the app with
//! an error, so a broken catalog never draws a silent substitute (charter §1.2).
//!
//! # Invariants
//! - No variant carries a string-typed cause where a typed one exists. Large causes are boxed
//!   (still typed), so a `Result` of this error stays small.

use super::schema::{Binding, EventKind};
use std::path::PathBuf;

/// Which named table of `effects.ron` a name was looked up in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// `ramps`.
    Ramp,
    /// `curves`.
    Curve,
    /// `emitters`.
    Emitter,
    /// `effects`.
    Effect,
}

impl std::fmt::Display for NameKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Ramp => "ramp",
            Self::Curve => "curve",
            Self::Emitter => "emitter",
            Self::Effect => "effect",
        })
    }
}

/// Where in `effects.ron` a problem is: an effect and, optionally, one of its layers (0-based).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// Name of the effect, or of the emitter, ramp or curve, or `defaults`.
    pub owner: String,
    /// Index of the layer inside the effect, when the problem is in one.
    pub layer: Option<usize>,
}

impl std::fmt::Display for Site {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.layer {
            Some(layer) => write!(f, "'{}' layer {layer}", self.owner),
            None => write!(f, "'{}'", self.owner),
        }
    }
}

/// A failure to load or resolve the effect catalog. Every variant names its file.
#[derive(Debug, thiserror::Error)]
pub enum EffectCatalogError {
    /// The file could not be read.
    #[error("cannot read '{path}': {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },
    /// The file is not valid RON for the schema.
    #[error("cannot parse '{path}': {source}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// Parser error with its position.
        #[source]
        source: Box<ron::error::SpannedError>,
    },
    /// The file's `format_version` is not the one this client reads.
    #[error("'{path}' has format_version {found}; this client reads {expected}")]
    FormatVersion {
        /// The file.
        path: PathBuf,
        /// Version in the file.
        found: u32,
        /// Version this client reads.
        expected: u32,
    },
    /// A ramp key has a colour or opacity channel outside 0 to 1. Effects are drawn without
    /// tone mapping on a camera with no HDR target, so a value above 1.0 only clips (plan
    /// D-11, decided in `WP6c`: no HDR and bloom; cores rely on additive layering and light).
    #[error(
        "'{path}': ramp '{ramp}' key {index} has a channel of {value}, outside 0 to 1 \
         (effects are not tone-mapped and the battle camera has no HDR, plan D-11)"
    )]
    RampOutOfRange {
        /// The file.
        path: PathBuf,
        /// Name of the ramp.
        ramp: String,
        /// Index of the key (0-based).
        index: usize,
        /// The offending value.
        value: f32,
    },
    /// A name does not resolve in its table.
    #[error("'{path}': {site} names unknown {kind} '{name}'")]
    UnknownName {
        /// The file.
        path: PathBuf,
        /// Where the name is used.
        site: Site,
        /// The table it was looked up in.
        kind: NameKind,
        /// The name.
        name: String,
    },
    /// A texture element is in none of the listed texture sets.
    #[error("'{path}': {site} names texture element '{element}', which no listed set has")]
    MissingTextureElement {
        /// The file.
        path: PathBuf,
        /// Where the element is used.
        site: Site,
        /// The element name.
        element: String,
    },
    /// A texture element exists but cannot be drawn by a block (a data or noise element).
    #[error(
        "'{path}': {site} draws texture element '{element}', which is data, not a drawable element"
    )]
    UndrawableElement {
        /// The file.
        path: PathBuf,
        /// Where the element is used.
        site: Site,
        /// The element name.
        element: String,
    },
    /// A `breakup` names an element that is not a noise element.
    #[error("'{path}': {site} reads texture element '{element}' as breakup noise, which is not a noise element")]
    NotNoiseElement {
        /// The file.
        path: PathBuf,
        /// Where the element is used.
        site: Site,
        /// The element name.
        element: String,
    },
    /// An emitter's `land` effect is not made for `Point` events.
    #[error(
        "'{path}': emitter '{emitter}' lands with effect '{effect}', whose event is not Point"
    )]
    LandEffectNotPoint {
        /// The file.
        path: PathBuf,
        /// The emitter.
        emitter: String,
        /// The effect it names.
        effect: String,
    },
    /// A ring with `mapping: Front` has no `breakup`.
    #[error("'{path}': {site} is a Front ring without the breakup noise it is computed from")]
    FrontWithoutBreakup {
        /// The file.
        path: PathBuf,
        /// The layer.
        site: Site,
    },
    /// Two texture sets define the same element name.
    #[error("'{path}': texture element '{element}' is defined by two sets")]
    DuplicateElement {
        /// The manifest that repeats it.
        path: PathBuf,
        /// The element name.
        element: String,
    },
    /// A texture manifest is unreadable or inconsistent.
    #[error("texture manifest '{path}': {source}")]
    TextureManifest {
        /// The manifest.
        path: PathBuf,
        /// The asset baker's verdict.
        #[source]
        source: Box<asset_baker::effects::EffectError>,
    },
    /// A texture strip could not be decoded.
    #[error("texture strip '{path}': {source}")]
    TextureImage {
        /// The strip.
        path: PathBuf,
        /// Decoder error.
        #[source]
        source: Box<image::ImageError>,
    },
    /// A texture strip does not have the size its manifest implies.
    #[error("texture strip '{path}' is {found:?} pixels; its manifest implies {expected:?}")]
    StripSize {
        /// The strip.
        path: PathBuf,
        /// Width and height found.
        found: [u32; 2],
        /// Width and height expected.
        expected: [u32; 2],
    },
    /// Drawable sets must share one cell size and mip count, because they share one array.
    #[error("texture manifest '{path}' has cells of {found} pixels and {found_mips} mips; earlier sets have {expected} and {expected_mips}")]
    CellSizeMismatch {
        /// The manifest.
        path: PathBuf,
        /// Its cell size.
        found: u32,
        /// Its mip count.
        found_mips: usize,
        /// The cell size of the earlier sets.
        expected: u32,
        /// The mip count of the earlier sets.
        expected_mips: usize,
    },
    /// A layer uses a binding its effect's event kind does not provide.
    #[error("'{path}': {site} binds {binding:?}, which a {event:?} event does not provide")]
    UnboundBinding {
        /// The file.
        path: PathBuf,
        /// The layer.
        site: Site,
        /// The binding.
        binding: Binding,
        /// The effect's event kind.
        event: EventKind,
    },
    /// Curve or ramp keys are empty, outside 0..=1 or not strictly increasing.
    #[error("'{path}': the keys of {site} are not strictly increasing in 0..=1 (key {index})")]
    UnorderedKeys {
        /// The file.
        path: PathBuf,
        /// The curve or ramp.
        site: Site,
        /// Index of the first offending key (0 when there are none).
        index: usize,
    },
    /// An effect has more layers than D-12 allows.
    #[error("'{path}': effect '{effect}' has {count} layers; at most {limit} are allowed")]
    TooManyLayers {
        /// The file.
        path: PathBuf,
        /// The effect.
        effect: String,
        /// Its layer count.
        count: usize,
        /// The limit.
        limit: usize,
    },
    /// A layer has more placements than D-12 allows.
    #[error("'{path}': {site} has {count} placements; at most {limit} are allowed")]
    TooManyPlacements {
        /// The file.
        path: PathBuf,
        /// The layer.
        site: Site,
        /// Its placement count.
        count: usize,
        /// The limit.
        limit: usize,
    },
    /// A placement replicates more instances than D-12 allows.
    #[error("'{path}': {site} places {count} instances; at most {limit} are allowed")]
    TooManyInstances {
        /// The file.
        path: PathBuf,
        /// The layer.
        site: Site,
        /// The instance count.
        count: u32,
        /// The limit.
        limit: u32,
    },
    /// A number is outside the range its field allows.
    #[error("'{path}': {site} field `{field}` is {value}; {rule}")]
    InvalidValue {
        /// The file.
        path: PathBuf,
        /// Where.
        site: Site,
        /// The field.
        field: &'static str,
        /// The value found.
        value: f32,
        /// The rule it breaks.
        rule: &'static str,
    },
    /// A block is used but the budget gives its pool no capacity.
    #[error("'{path}': {site} uses the {block} pool, whose budget capacity is 0")]
    NoCapacity {
        /// The file.
        path: PathBuf,
        /// The first layer that uses it.
        site: Site,
        /// The pool.
        block: &'static str,
    },
    /// A unit's presentation block names an effect the vocabulary does not have.
    #[error("unit '{unit}' {slot} names unknown effect '{name}' (vocabulary '{path}')")]
    UnknownUnitEffect {
        /// The vocabulary file.
        path: PathBuf,
        /// Slug of the unit or structure.
        unit: String,
        /// Which entry of its block.
        slot: String,
        /// The effect name.
        name: String,
    },
    /// A unit's presentation block names a mode the archetype does not declare.
    #[error("unit '{unit}' presentation block names unknown mode '{mode}'")]
    UnknownUnitMode {
        /// Slug of the unit or structure.
        unit: String,
        /// The mode name.
        mode: String,
    },
    /// A unit's presentation block uses an effect made for another kind of event.
    #[error(
        "unit '{unit}' {slot} names effect '{name}' made for {found:?} events, not {expected:?}"
    )]
    WrongEventKind {
        /// Slug of the unit or structure.
        unit: String,
        /// Which entry of its block.
        slot: String,
        /// The effect name.
        name: String,
        /// The kinds the entry accepts.
        expected: &'static [EventKind],
        /// The effect's kind.
        found: EventKind,
    },
}
