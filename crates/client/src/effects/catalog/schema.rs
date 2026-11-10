//! The on-disk schema of `assets/catalog/effects.ron` (plan `2026-11-09-effect-visualization`
//! §5.2), as `serde` reads it.
//!
//! # Responsibility
//! Declares the authored shapes only: names are still strings here. [`super::resolve`] turns
//! them into the dense, index-based [`super::EffectCatalog`] the runtime uses.
//!
//! # How it fits
//! The file is read with RON's `implicit_some` extension, so an optional field is written as a
//! plain value (`erosion: "rise"`), the way the plan's sketch writes it.
//!
//! # Invariants
//! - Every struct refuses unknown fields, so a typo is a parse error naming its position.
//! - Times are seconds of presentation time; lengths are world units; angles are degrees.

use serde::Deserialize;
use sim_types::StatusKind;
use std::collections::BTreeMap;

/// The whole file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectsFile {
    /// Schema version; [`super::EFFECTS_FORMAT_VERSION`].
    pub format_version: u32,
    /// Texture-set manifests (`<set>.texture.ron`), relative to the assets root or absolute.
    #[serde(default)]
    pub textures: Vec<String>,
    /// Colour ramps by name: `(position 0..1, (r, g, b, a))` keys, linear premultiplied-ready
    /// RGB (values above 1 are HDR cores) and opacity, where opacity 0 is additive.
    #[serde(default)]
    pub ramps: BTreeMap<String, Vec<RampKey>>,
    /// Timing curves by name: `(time 0..1, value, easing to the next key)`.
    #[serde(default)]
    pub curves: BTreeMap<String, Vec<(f32, f32, Easing)>>,
    /// Particle emitters by name.
    #[serde(default)]
    pub emitters: BTreeMap<String, EmitterDef>,
    /// Effects by name.
    #[serde(default)]
    pub effects: BTreeMap<String, EffectDefRaw>,
    /// Pool capacities and level-of-detail rules.
    pub budget: BudgetDef,
    /// Effects used where a unit's presentation block names none.
    #[serde(default)]
    pub defaults: DefaultsDef,
}

/// One ramp key: position 0..1 and `(r, g, b, opacity)`.
pub type RampKey = (f32, (f32, f32, f32, f32));

/// Easing from one curve key to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Easing {
    /// Straight line.
    Linear,
    /// Holds the key's value until the next key.
    Step,
    /// Slow start, `t²`.
    EaseInQuad,
    /// Fast start, `1 - (1 - t)²`.
    EaseOutQuad,
    /// Fast start, `1 - (1 - t)³`.
    EaseOutCubic,
    /// Slow start and end, smoothstep.
    EaseInOut,
}

/// Budget class of an effect (§5.6): each class has a live-count cap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize)]
pub enum EffectClass {
    /// Large, rare, gameplay-relevant effects; never retired for others.
    Major,
    /// Frequent small effects; their detail is retired first under pressure.
    Minor,
    /// Background effects; their detail is retired first under pressure.
    Ambient,
}

impl EffectClass {
    /// Every class, in index order.
    pub const ALL: [Self; 3] = [Self::Major, Self::Minor, Self::Ambient];

    /// Dense index of the class.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Major => 0,
            Self::Minor => 1,
            Self::Ambient => 2,
        }
    }
}

/// The kind of event an effect is made for, which decides the bindings it may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EventKind {
    /// One splash search (`SplashVisual`): zone radii.
    Splash,
    /// One point of a line splash: zone radii and the line.
    Line,
    /// A weapon's impact point (tracer end, beam end, projectile arrival, melee contact).
    Point,
    /// A persistent area (`RenderSnapshot::areas`).
    Area,
    /// A tether between two units.
    Link,
    /// A status on a unit.
    Status,
    /// A mode switch or stance of a unit.
    Mode,
    /// A unit's death.
    Death,
}

/// A value the simulation publishes for an event (§5.4), multiplied by a layer's `scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Binding {
    /// The constant 1: `(scale: 2.0)` is the constant 2.
    #[default]
    Const,
    /// Radius of the innermost (full damage) zone.
    SplashFull,
    /// Radius of the middle zone.
    SplashMid,
    /// Radius of the outermost zone (reach).
    SplashReach,
    /// Length of the whole line.
    LineLength,
    /// Distance between search points of the line.
    LineSpacing,
    /// Radius of a persistent area.
    AreaRadius,
    /// Seconds an area still lasts.
    AreaRemaining,
    /// Seconds an area lasts in all.
    AreaTotal,
    /// Length of a tether.
    LinkLength,
    /// The unit's bubble radius.
    UnitRadius,
    /// The unit's bubble height.
    UnitHeight,
    /// Progress 0..1 of a mode switch.
    ChannelProgress,
    /// Fraction 0..1 of the barrier left.
    BarrierRatio,
    /// Flight time of a projectile, in seconds.
    FlightTime,
}

/// A size, radius or spacing: a binding times a scale.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValueDef {
    /// The bound value; [`Binding::Const`] is 1.
    #[serde(default)]
    pub bind: Binding,
    /// Factor applied to the bound value.
    #[serde(default = "one")]
    pub scale: f32,
}

/// Serde default of a scale.
const fn one() -> f32 {
    1.0
}

/// Serde default of a frame blend.
const fn yes() -> bool {
    true
}

/// A radius animated from one value to another over the layer's life.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RadiusDef {
    /// Radius at the start of the layer.
    pub from: ValueDef,
    /// Radius at the end of the layer.
    pub to: ValueDef,
    /// Progress from `from` to `to` over the layer's life (0 at `from`, 1 at `to`); `None`
    /// is linear. A curve that reaches 1 early holds the ring at `to`, a front that stops at
    /// the reach.
    #[serde(default)]
    pub curve: Option<String>,
}

/// How a card is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Orient {
    /// Faces the camera.
    #[default]
    Camera,
    /// Lies flat on the ground plane.
    Ground,
    /// Stands upright, its width along the event's line (or facing the camera without one).
    VerticalAlongLine,
}

/// How a card's roll angle and mirroring are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum RotationDef {
    /// No roll, never mirrored.
    #[default]
    Fixed,
    /// A roll and a horizontal mirror drawn from the layer's deterministic hash.
    Random,
    /// No roll; a horizontal mirror drawn from the hash (upright elements such as spikes,
    /// which must keep their base down).
    Mirror,
}

/// How a ring card maps its texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum RingMapping {
    /// The texture spans the card; the ring mask cuts the band out of it.
    #[default]
    Card,
    /// The texture's middle band is wrapped around the ring (v across the band, u along the
    /// angle, mirrored back and forth over the element's middle so there is no seam), so any
    /// element breaks the ring up along its length.
    Polar,
}

/// How a decal's box is turned about world up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum DecalYaw {
    /// A yaw drawn from the layer's deterministic hash.
    #[default]
    Random,
    /// The texture's u axis along the event's line (random without a line).
    AlongLine,
}

/// The block a layer draws, with its look parameters (§5.1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum BlockDef {
    /// A flipbook or still card.
    Sprite {
        /// Texture element.
        texture: String,
        /// Colour ramp.
        ramp: String,
        /// Card orientation.
        #[serde(default)]
        orient: Orient,
        /// Soft-depth fade distance in world units; 0 for a hard intersection.
        #[serde(default)]
        soft_depth: f32,
        /// Whether consecutive flipbook frames are blended.
        #[serde(default = "yes")]
        frame_blend: bool,
        /// Roll angle choice.
        #[serde(default)]
        rotation: RotationDef,
        /// Erosion threshold over the layer's life (0 none, 1 all eroded).
        #[serde(default)]
        erosion: Option<String>,
        /// Drift of the card in world units per second, `(x, up, z)`.
        #[serde(default)]
        drift: (f32, f32, f32),
        /// Flipbook frames per second; `None` fits the flipbook to the layer's duration.
        #[serde(default)]
        fps: Option<f32>,
        /// Flipbook position over the layer's life (0 first frame, 1 last frame); overrides
        /// `fps`. A falling curve plays the flipbook backwards, a flat one holds a frame.
        #[serde(default)]
        frame_curve: Option<String>,
        /// Height of the card's centre above its placed point, in multiples of its half
        /// size; `None` is 1 for `VerticalAlongLine` (base on the ground) and 0 otherwise.
        #[serde(default)]
        lift: Option<f32>,
    },
    /// A card masked to a ring, its radius animated from `radius.from` to `radius.to`.
    Ring {
        /// Texture element (its shading breaks the ring up).
        texture: String,
        /// Colour ramp.
        ramp: String,
        /// Card orientation.
        #[serde(default)]
        orient: Orient,
        /// Ring thickness as a fraction of the radius, in (0, 1].
        thickness: f32,
        /// Erosion threshold over the layer's life.
        #[serde(default)]
        erosion: Option<String>,
        /// How the texture is mapped onto the ring.
        #[serde(default)]
        mapping: RingMapping,
        /// Height of the ring above its placed point in world units (a ground ring sits a
        /// little above the ground so the depth test keeps it).
        #[serde(default)]
        height: f32,
    },
    /// A ground mark projected through the depth buffer.
    Decal {
        /// Texture element.
        texture: String,
        /// Colour ramp.
        ramp: String,
        /// Erosion threshold over the layer's life.
        #[serde(default)]
        erosion: Option<String>,
        /// Half height of the projection box in world units.
        #[serde(default = "one")]
        depth: f32,
        /// Upward-normal cosine below which the mark fades out (steep faces), in [0, 1).
        #[serde(default = "default_grazing")]
        grazing: f32,
        /// How the box is turned about world up.
        #[serde(default)]
        yaw: DecalYaw,
        /// Half width across the texture's u axis as a fraction of the half length (`size`),
        /// in (0, 1]; 1 is a square mark.
        #[serde(default = "one")]
        width: f32,
        /// With `yaw: AlongLine`, the mark is cut off where the event's line starts and ends,
        /// so segments longer than their spacing still end exactly at the line's ends.
        #[serde(default)]
        clip_to_line: bool,
    },
    /// A short point light.
    Light {
        /// Linear RGB colour.
        color: (f32, f32, f32),
        /// Peak intensity in lumens.
        intensity: f32,
        /// Height of the light above its placed point in world units (a light on the ground
        /// grazes it and lights nothing).
        #[serde(default)]
        height: f32,
    },
    /// A burst of CPU-simulated particles.
    Particles {
        /// Emitter name.
        emitter: String,
    },
}

/// Serde default of a decal's grazing cosine.
const fn default_grazing() -> f32 {
    0.35
}

/// Whether a layer may be dropped by the budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Priority {
    /// Carries gameplay information; never dropped.
    Critical,
    /// Decoration; dropped or retired under pressure.
    #[default]
    Detail,
}

/// Where a layer's instances are placed around the event's anchor.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum PlacementDef {
    /// One instance at the anchor.
    Point,
    /// `count` instances scattered in a disc of `radius`.
    Disc {
        /// Instances.
        count: u32,
        /// Disc radius.
        radius: ValueDef,
    },
    /// Instances every `spacing` along the event's line, each delayed by its distance
    /// divided by `travel` (world units per second; 0 for no delay).
    Line {
        /// Distance between instances.
        spacing: ValueDef,
        /// Speed at which the instances appear along the line.
        #[serde(default)]
        travel: f32,
        /// Instances sit between the points, half a spacing in, one fewer: segments that join
        /// the points and end exactly at the line's ends (rather than centred on the ends).
        #[serde(default)]
        between: bool,
    },
}

/// One layer: a block, its timing, size and placements.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayerDef {
    /// The block and its look.
    pub block: BlockDef,
    /// Delay from the event, in seconds.
    #[serde(default)]
    pub start: f32,
    /// Life of the layer in seconds (ignored by `Particles`, which live by their emitter).
    #[serde(default)]
    pub duration: f32,
    /// Intensity over the layer's life; `None` is a constant 1.
    #[serde(default)]
    pub curve: Option<String>,
    /// Factor on `size` over the layer's life (cards, decals and lights); `None` is 1.
    #[serde(default)]
    pub size_curve: Option<String>,
    /// Factor on the texture's shading before the ramp lookup over the layer's life (cards
    /// and decals): a falling curve cools the element down its ramp; `None` is 1.
    #[serde(default)]
    pub ramp_curve: Option<String>,
    /// Random spread of each placed instance's size: the size is scaled by a hashed factor in
    /// `[1 - size_jitter, 1 + size_jitter]`; in [0, 1).
    #[serde(default)]
    pub size_jitter: f32,
    /// Budget priority.
    #[serde(default)]
    pub priority: Priority,
    /// Card half-size, decal radius, light radius or emitter radius.
    #[serde(default)]
    pub size: Option<ValueDef>,
    /// Ring radius over life (rings only).
    #[serde(default)]
    pub radius: Option<RadiusDef>,
    /// A single placement (shorthand for `placements: [..]`).
    #[serde(default)]
    pub placement: Option<PlacementDef>,
    /// Placements; empty means one `Point`.
    #[serde(default)]
    pub placements: Vec<PlacementDef>,
}

/// An effect: a class, an event kind and its layers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EffectDefRaw {
    /// Budget class.
    pub class: EffectClass,
    /// The kind of event it is made for.
    pub event: EventKind,
    /// Its layers, at most 12.
    pub layers: Vec<LayerDef>,
}

/// How many particles a burst spawns: `base + per_unit_radius · radius`, at most `max`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BurstCount {
    /// Particles at radius 0.
    pub base: u32,
    /// Extra particles per world unit of the layer's size.
    #[serde(default)]
    pub per_unit_radius: f32,
    /// Upper bound.
    pub max: u32,
}

/// When an emitter spawns.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum SpawnDef {
    /// All particles at the layer's start.
    Burst {
        /// How many.
        count: BurstCount,
    },
}

/// Where particles start, relative to the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum EmitterShape {
    /// At the anchor.
    #[default]
    Point,
    /// Inside a ground disc of the layer's size.
    Disc,
    /// On the rim of a ground circle of the layer's size.
    Ring,
}

/// How a particle card is turned.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum ParticleOrient {
    /// Faces the camera.
    Camera,
    /// Faces the camera, stretched along its velocity by `1 + factor · speed · 0.1`.
    VelocityStretched(f32),
    /// Lies flat on the ground.
    Ground,
}

/// A particle emitter definition.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitterDef {
    /// Spawn rule.
    pub spawn: SpawnDef,
    /// Start shape.
    #[serde(default)]
    pub shape: EmitterShape,
    /// Initial speed range, world units per second.
    pub speed: (f32, f32),
    /// Half angle in degrees of the launch cone around world up.
    pub cone_deg: f32,
    /// Downward acceleration, world units per second².
    #[serde(default)]
    pub gravity: f32,
    /// Linear drag per second.
    #[serde(default)]
    pub drag: f32,
    /// Life range in seconds.
    pub lifetime: (f32, f32),
    /// Card half-size range in world units.
    pub size: (f32, f32),
    /// Texture element.
    pub texture: String,
    /// Card orientation.
    pub orient: ParticleOrient,
    /// Colour ramp, read along the particle's life.
    pub ramp: String,
    /// Size over life; `None` keeps the start size.
    #[serde(default)]
    pub size_curve: Option<String>,
    /// Opacity over life; `None` is a constant 1.
    #[serde(default)]
    pub curve: Option<String>,
    /// Roll of each card: `Fixed` uses `roll_deg` only, `Random` adds a hashed roll and
    /// mirror, `Mirror` a hashed mirror.
    #[serde(default)]
    pub rotation: RotationDef,
    /// Fixed roll in degrees, which turns the texture's own axis onto the card's length (90
    /// for an element drawn along +x under `VelocityStretched`).
    #[serde(default)]
    pub roll_deg: f32,
}

/// Pool capacities and level-of-detail rules (§5.6), sized at load and never grown.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetDef {
    /// Live particles.
    pub particles: u32,
    /// Sprite and ring cards per frame (particles are counted separately).
    pub sprites: u32,
    /// Decals per frame.
    pub decals: u32,
    /// Pooled point lights.
    #[serde(default)]
    pub lights: u32,
    /// Live effect instances (slots of the instance pool).
    pub effects: u32,
    /// Live caps per class (`{ Major: 96, Minor: 256, Ambient: 128 }`); a class left out has
    /// cap 0 and no effect of it may be defined.
    pub classes: BTreeMap<EffectClass, u32>,
    /// Layers whose projected size is below this many pixels are skipped.
    #[serde(default = "default_min_screen_px")]
    pub min_screen_px: f32,
    /// Projected size in pixels at which a burst spawns all its particles.
    #[serde(default = "default_full_particle_px")]
    pub full_particle_px: f32,
    /// Live on-screen `Major` effects above which detail particle counts halve.
    #[serde(default = "default_crowd_threshold")]
    pub crowd_threshold: u32,
}

/// Serde default of `min_screen_px`.
const fn default_min_screen_px() -> f32 {
    6.0
}

/// Serde default of `full_particle_px`.
const fn default_full_particle_px() -> f32 {
    96.0
}

/// Serde default of `crowd_threshold`.
const fn default_crowd_threshold() -> u32 {
    24
}

/// Effects used where a presentation block names none; every entry optional.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefaultsDef {
    /// A weapon's impact.
    #[serde(default)]
    pub impact: Option<String>,
    /// A weapon's splash search.
    #[serde(default)]
    pub splash: Option<String>,
    /// A unit's death.
    #[serde(default)]
    pub death: Option<String>,
    /// A status, by kind.
    #[serde(default)]
    pub status: BTreeMap<StatusKind, String>,
}
