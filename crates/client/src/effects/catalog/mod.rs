//! The effect vocabulary (`assets/catalog/effects.ron`), resolved into dense tables.
//!
//! # Responsibility
//! Owns [`EffectCatalog`]: textures, ramps, curves, emitters, effects, the budget and the
//! defaults, every name replaced by a dense index at load (plan §5.2, §5.6 "Load"). Also owns
//! [`UnitEffectTables`], the per-unit choice of effects read from each archetype's
//! presentation block (§5.3).
//!
//! # How it fits
//! Read from disk at run time (charter C6): a changed file draws a different effect with no
//! rebuild. The runtime and the renderer index these tables; no Rust names an effect (C5).
//!
//! # Invariants
//! - Every name resolves, every binding is provided by its effect's event kind, every curve
//!   and ramp is ordered, and the D-12 limits hold; otherwise loading fails with an
//!   [`EffectCatalogError`] naming the file, effect and layer.
//! - Every pool an effect uses has a nonzero capacity in the budget.

pub mod error;
mod resolve;
pub mod schema;
mod textures;
mod units;

pub use error::{EffectCatalogError, NameKind, Site};
pub use textures::{AtlasElement, TextureAtlas};
pub use units::{UnitEffectTables, WeaponEffectIds};

use super::curve::{Curve, Ramp};
use bevy::prelude::*;
use schema::{
    BurstCount, DecalYaw, EffectClass, EmitterShape, EventKind, Orient, ParticleOrient,
    PlacementDef, Priority, RingMapping, RotationDef, ValueDef,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The `format_version` this client reads.
pub const EFFECTS_FORMAT_VERSION: u32 = 1;
/// D-12: the most layers one effect has.
pub const MAX_LAYERS: usize = 12;
/// D-12: the most placements one layer has.
pub const MAX_PLACEMENTS: usize = 4;
/// Path of the shipped vocabulary below the assets root.
pub const EFFECTS_CATALOG_PATH: &str = "catalog/effects.ron";

/// Dense index of an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EffectId(pub u16);

/// Dense index of a ramp row in the ramp atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RampId(pub u16);

/// Dense index of a curve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CurveId(pub u16);

/// Dense index of an emitter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmitterId(pub u16);

/// A drawable texture element: where its frames are in the texture array.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElementRef {
    /// Array layer of the first frame.
    pub first_layer: u32,
    /// Frames (1 for a still).
    pub frames: u32,
    /// Real play time of all frames in seconds, for a timed flipbook.
    pub duration_secs: Option<f32>,
}

impl ElementRef {
    /// Whether the element is a set of interchangeable variants (several frames and no play
    /// time) rather than a flipbook: each drawn instance shows one variant, chosen by hash.
    #[must_use]
    pub const fn is_variant_set(&self) -> bool {
        self.frames > 1 && self.duration_secs.is_none()
    }
}

/// The look of a card (sprite or ring).
#[derive(Debug, Clone, PartialEq)]
pub struct CardLook {
    /// Texture element.
    pub element: ElementRef,
    /// Ramp row.
    pub ramp: RampId,
    /// Orientation.
    pub orient: Orient,
    /// Soft-depth distance in world units; 0 is a hard intersection.
    pub soft_depth: f32,
    /// Whether consecutive frames are blended.
    pub frame_blend: bool,
    /// Roll choice.
    pub rotation: RotationDef,
    /// Erosion over life.
    pub erosion: Option<CurveId>,
    /// Drift in world units per second.
    pub drift: Vec3,
    /// Frames per second; `None` fits the flipbook to the layer.
    pub fps: Option<f32>,
    /// Flipbook position over life; overrides `fps`.
    pub frame_curve: Option<CurveId>,
    /// Height of the card's centre above its placed point in half sizes; `None` is the
    /// orientation's default.
    pub lift: Option<f32>,
    /// Texture mapping of a ring (plain cards ignore it).
    pub mapping: RingMapping,
}

/// The look of a decal.
#[derive(Debug, Clone, PartialEq)]
pub struct DecalLook {
    /// Texture element.
    pub element: ElementRef,
    /// Ramp row.
    pub ramp: RampId,
    /// Erosion over life.
    pub erosion: Option<CurveId>,
    /// Half height of the projection box, world units.
    pub depth: f32,
    /// Upward-normal cosine below which the mark fades out.
    pub grazing: f32,
    /// How the box is turned about world up.
    pub yaw: DecalYaw,
    /// Half width as a fraction of the half length.
    pub width: f32,
    /// Whether the mark is cut off at the line's ends.
    pub clip_to_line: bool,
}

/// A layer's block, resolved.
#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    /// A card.
    Sprite(CardLook),
    /// A card masked to a ring of relative `thickness`, radius from the layer's `radius`.
    Ring {
        /// The card's look.
        look: CardLook,
        /// Thickness as a fraction of the radius.
        thickness: f32,
        /// Height above the placed point, world units.
        height: f32,
    },
    /// A projected ground mark.
    Decal(DecalLook),
    /// A pooled point light.
    Light {
        /// Linear RGB.
        color: Vec3,
        /// Peak intensity in lumens.
        intensity: f32,
        /// Height above the placed point, world units.
        height: f32,
    },
    /// A particle burst.
    Particles(EmitterId),
}

/// The block pools of the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pool {
    /// Sprite and ring cards.
    Sprites,
    /// Decals.
    Decals,
    /// Particles.
    Particles,
    /// Point lights.
    Lights,
}

impl Block {
    /// The pool the block draws from.
    #[must_use]
    pub const fn pool(&self) -> Pool {
        match self {
            Self::Sprite(_) | Self::Ring { .. } => Pool::Sprites,
            Self::Decal(_) => Pool::Decals,
            Self::Light { .. } => Pool::Lights,
            Self::Particles(_) => Pool::Particles,
        }
    }
}

/// A resolved layer.
#[derive(Debug, Clone, PartialEq)]
pub struct Layer {
    /// The block.
    pub block: Block,
    /// Delay from the event, seconds.
    pub start: f32,
    /// Life in seconds (for particles: the longest particle life).
    pub duration: f32,
    /// Intensity over life.
    pub curve: Option<CurveId>,
    /// Budget priority.
    pub priority: Priority,
    /// Half-size, radius or emitter radius.
    pub size: ValueDef,
    /// Factor on the size over life.
    pub size_curve: Option<CurveId>,
    /// Factor on the shading before the ramp lookup over life.
    pub ramp_curve: Option<CurveId>,
    /// Hashed spread of each instance's size, as a fraction.
    pub size_jitter: f32,
    /// Ring radius over life.
    pub radius: Option<RingRadius>,
    /// Placements, 1 to [`MAX_PLACEMENTS`].
    pub placements: Vec<PlacementDef>,
}

/// A ring's radius over its layer's life, resolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RingRadius {
    /// Radius at the start.
    pub from: ValueDef,
    /// Radius at the end.
    pub to: ValueDef,
    /// Progress from `from` to `to` over life; `None` is linear.
    pub curve: Option<CurveId>,
}

/// A resolved effect.
#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    /// Its name in the file.
    pub name: String,
    /// Budget class.
    pub class: EffectClass,
    /// Event kind.
    pub event: EventKind,
    /// Layers, at most [`MAX_LAYERS`].
    pub layers: Vec<Layer>,
    /// Seconds from the event until the last layer ends.
    pub lifetime: f32,
}

/// A resolved emitter.
#[derive(Debug, Clone, PartialEq)]
pub struct Emitter {
    /// Burst size rule.
    pub count: BurstCount,
    /// Start shape.
    pub shape: EmitterShape,
    /// Speed range, world units per second.
    pub speed: (f32, f32),
    /// Launch cone half angle, radians.
    pub cone: f32,
    /// Downward acceleration.
    pub gravity: f32,
    /// Linear drag per second.
    pub drag: f32,
    /// Life range, seconds.
    pub lifetime: (f32, f32),
    /// Half-size range, world units.
    pub size: (f32, f32),
    /// Texture element.
    pub element: ElementRef,
    /// Orientation.
    pub orient: ParticleOrient,
    /// Ramp row.
    pub ramp: RampId,
    /// Size over life.
    pub size_curve: Option<CurveId>,
    /// Opacity over life.
    pub curve: Option<CurveId>,
    /// Roll and mirror choice.
    pub rotation: RotationDef,
    /// Fixed roll, radians.
    pub roll: f32,
}

/// Pool capacities and level-of-detail rules.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// Live particles.
    pub particles: u32,
    /// Cards per frame.
    pub sprites: u32,
    /// Decals per frame.
    pub decals: u32,
    /// Pooled lights.
    pub lights: u32,
    /// Live effect instances.
    pub effects: u32,
    /// Live caps by [`EffectClass::index`].
    pub class_caps: [u32; 3],
    /// Smallest projected size drawn, pixels.
    pub min_screen_px: f32,
    /// Projected size at which bursts spawn every particle, pixels.
    pub full_particle_px: f32,
    /// On-screen `Major` effects above which detail particles halve.
    pub crowd_threshold: u32,
}

/// Defaults by slot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Defaults {
    /// A weapon's impact.
    pub impact: Option<EffectId>,
    /// A weapon's splash.
    pub splash: Option<EffectId>,
    /// A death.
    pub death: Option<EffectId>,
    /// Statuses, by `StatusKind`.
    pub status: BTreeMap<sim_types::StatusKind, EffectId>,
}

/// The resolved effect vocabulary.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct EffectCatalog {
    /// The file it was read from.
    pub path: PathBuf,
    /// Texture elements and the drawable texel array.
    pub atlas: TextureAtlas,
    /// Ramp rows, in [`RampId`] order.
    pub ramps: Vec<Ramp>,
    /// Curves, in [`CurveId`] order.
    pub curves: Vec<Curve>,
    /// Emitters, in [`EmitterId`] order.
    pub emitters: Vec<Emitter>,
    /// Effects, in [`EffectId`] order.
    pub effects: Vec<Effect>,
    /// Effect names to ids.
    pub effect_names: BTreeMap<String, EffectId>,
    /// Pools and LOD rules.
    pub budget: Budget,
    /// Defaults.
    pub defaults: Defaults,
}

impl EffectCatalog {
    /// Reads and resolves the vocabulary at `path`; texture manifests are relative to
    /// `assets_root` unless absolute.
    ///
    /// # Errors
    /// Any [`EffectCatalogError`].
    pub fn load(path: &Path, assets_root: &Path) -> Result<Self, EffectCatalogError> {
        let text = std::fs::read_to_string(path).map_err(|source| EffectCatalogError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_text(&text, path, assets_root)
    }

    /// Resolves the vocabulary in `text`, reported as coming from `path`.
    ///
    /// # Errors
    /// Any [`EffectCatalogError`].
    pub fn from_text(
        text: &str,
        path: &Path,
        assets_root: &Path,
    ) -> Result<Self, EffectCatalogError> {
        let options = ron::Options::default()
            .with_default_extension(ron::extensions::Extensions::IMPLICIT_SOME);
        let file: schema::EffectsFile =
            options
                .from_str(text)
                .map_err(|source| EffectCatalogError::Parse {
                    path: path.to_path_buf(),
                    source: Box::new(source),
                })?;
        resolve::resolve(&file, path, assets_root)
    }

    /// The effect named `name`.
    #[must_use]
    pub fn effect_id(&self, name: &str) -> Option<EffectId> {
        self.effect_names.get(name).copied()
    }

    /// The effect with id `id`.
    #[must_use]
    pub fn effect(&self, id: EffectId) -> Option<&Effect> {
        self.effects.get(usize::from(id.0))
    }

    /// The curve `id` sampled at `t`, or `fallback` without a curve.
    #[must_use]
    pub fn sample(&self, id: Option<CurveId>, t: f32, fallback: f32) -> f32 {
        id.and_then(|id| self.curves.get(usize::from(id.0)))
            .map_or(fallback, |curve| curve.sample(t))
    }
}
