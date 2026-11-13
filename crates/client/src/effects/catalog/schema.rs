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

/// Serde default of a count of one.
const fn one_u32() -> u32 {
    1
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
    /// Stands upright and turns about world up to face the camera (a cylindrical
    /// billboard): things standing on the ground (spines, columns) never turn edge-on.
    Upright,
}

/// The shape a card draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum CardShape {
    /// The texture element.
    #[default]
    Texture,
    /// A soft radial glow computed in the shader (shading and coverage fall off as a
    /// Gaussian from the centre; the erosion order rises towards the centre), broken by the
    /// layer's `breakup` noise: flashes and glows, which are light rather than matter.
    Radial,
    /// A thin glassy membrane round a body, computed in the shader (plan WP9, `barrier_shell`):
    /// the card is the sphere's outline seen from the camera, faint where the view meets the
    /// membrane square on and thickest towards its edge, as light through a thin shell is, with
    /// a soft outer edge; the layer's `breakup` noise breaks it up. Only a `Sprite` may draw it.
    Membrane,
}

/// Noise breakup of an element's erosion and edges, with a phase of its own per instance, so
/// no two instances erode alike and no erosion is mirror-symmetric.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BreakupDef {
    /// Noise element (a `NoiseV1` set element; its first channel is read).
    pub noise: String,
    /// Noise repeats across the element (a ring: around its circumference, rounded to whole
    /// repeats so it has no seam).
    #[serde(default = "one")]
    pub scale: f32,
    /// Weight of the noise in the erosion order, in [0, 1]: 0 erodes by the element's baked
    /// order alone, 1 by the noise alone.
    #[serde(default)]
    pub amount: f32,
    /// Width of the soft step of the erosion threshold, in (0, 1]: wider is softer.
    #[serde(default = "default_softness")]
    pub softness: f32,
    /// Self-shading in [0, 1]: thin parts of the element are darkened towards its dark ramp
    /// end by this much (volume: dense middles lit, thin edges in shadow, no bright rims).
    #[serde(default)]
    pub shade: f32,
}

/// Serde default of an erosion softness.
const fn default_softness() -> f32 {
    0.08
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
    /// A pressure front computed from the layer's `breakup` noise (which it requires):
    /// brightest and densest at its leading (outer) edge, fading towards the trailing edge,
    /// its width and brightness wavering around the circumference with a phase of its own per
    /// instance; the element's texture is not read.
    Front,
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
        /// size; `None` is 1 for `VerticalAlongLine` and `Upright` (base on the ground) and 0
        /// otherwise.
        #[serde(default)]
        lift: Option<f32>,
        /// Lift over the layer's life, in half sizes (overrides `lift`): a card rising out of
        /// the ground and sinking back into it (the depth test hides what is below).
        #[serde(default)]
        lift_curve: Option<String>,
        /// Largest lean in degrees: each instance rolls by a hashed angle in `[-lean, lean]`
        /// about the point where it meets the ground (`Upright` and `VerticalAlongLine`) or its
        /// centre.
        #[serde(default)]
        lean_deg: f32,
        /// Distance in world units the card moves away from the effect's anchor over its life,
        /// easing out (a dust ring kicked outward).
        #[serde(default)]
        spread: f32,
        /// Moves the card this many world units towards the camera in the back-to-front sort
        /// only (a front drawn over the fireball standing on it).
        #[serde(default)]
        sort_bias: f32,
        /// With an event line, the card is cut off where the line starts and ends.
        #[serde(default)]
        clip_to_line: bool,
        /// Texture or computed shape.
        #[serde(default)]
        shape: CardShape,
        /// Noise breakup of the erosion and edges.
        #[serde(default)]
        breakup: Option<BreakupDef>,
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
        /// Moves the ring this many world units towards the camera in the back-to-front sort
        /// only.
        #[serde(default)]
        sort_bias: f32,
        /// Noise breakup of the erosion and edges (required by `mapping: Front`).
        #[serde(default)]
        breakup: Option<BreakupDef>,
        /// `mapping: Front` only: how deep the leading edge is bitten by its own noise, as a
        /// fraction of the band's width, in [0, 1). 0 keeps a round leading edge (a pressure
        /// front); above it the edge recedes where the noise is low, by up to this share of the
        /// band, so the front is a run of clumps whose farthest parts stay on the radius and
        /// whose outline differs per instance (a dust skirt; scale-out review 2). The footprint
        /// test allows its rays to end inside the reach by this bite.
        #[serde(default)]
        edge_noise: f32,
        /// Share of the full turn the ring draws from its start angle round, in [0, 1] once
        /// resolved (a mode switch's progress through `ChannelProgress`: a seal that closes as
        /// the switch completes, plan WP8). `None` draws the whole ring. The arc's head is a
        /// soft step one hundredth of a turn wide.
        #[serde(default)]
        arc: Option<ValueDef>,
        /// A `Card` or `Polar` ring only: how much its band wavers round the turn, in [0, 1). Its
        /// width varies by up to this share either way and its radius by a tenth of it, both
        /// from its breakup noise, and its colour runs from its ramp's low end on the inner
        /// edge to its high end on the outer one (a furrow and the soil thrown up beside it,
        /// WP8 review 1, B-5). 0 keeps an even band.
        #[serde(default)]
        waver: f32,
        /// A `Ground` ring only: the ring darkens the ground under it, multiplying it by its
        /// colour where it covers it, and never lightens it, shadows included (a furrow in
        /// the shade of its unit; WP8 review 2, B2-2). Its ramp then reads as the colour the
        /// ground is multiplied by.
        #[serde(default)]
        darken: bool,
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
        /// Half width across the texture's u axis as a multiple of the half length (`size`),
        /// in (0, 4]; 1 is a square mark, above 1 a box wider than long (for an element whose
        /// mark is a narrow band across it).
        #[serde(default = "one")]
        width: f32,
        /// With `yaw: AlongLine`, the mark is cut off where the event's line starts and ends,
        /// so segments longer than their spacing still end exactly at the line's ends.
        #[serde(default)]
        clip_to_line: bool,
        /// World units over which a mark cut off at the line's ends feathers out across each end
        /// (60 % of the fade beyond the end, 40 % inside the line), at least 0.005.
        #[serde(default = "default_clip_feather")]
        clip_feather: f32,
        /// Noise breakup of the erosion and edges.
        #[serde(default)]
        breakup: Option<BreakupDef>,
        /// Draws the mark as a wet liquid (a stain): see [`LiquidDef`].
        #[serde(default)]
        liquid: Option<LiquidDef>,
        /// Draws the mark inside a disc of the box's half length with a shallow noise edge,
        /// instead of the texture's own outline: see [`OutlineDef`].
        #[serde(default)]
        outline: Option<OutlineDef>,
        /// The mark darkens the ground under it, multiplying it by its colour where it covers
        /// it, and never lightens it, shadows included (soil pressed into the shade of its
        /// unit; WP8 review 2). Its ramp then reads as the colour the ground is multiplied
        /// by. A liquid cannot darken.
        #[serde(default)]
        darken: bool,
    },
    /// A strip along the event's line (plan WP7): a flame jet's core, a lance's burning
    /// stroke. Its half width is the layer's `size`; it runs from the line's first search
    /// point towards its last, its head racing out at `travel` and, after `tail_delay`, its
    /// tail following at the same speed, so it can sweep along the line as a short segment.
    /// The texture's u axis runs along the strip, repeating every `repeat` world units and
    /// scrolling by `scroll` repeats per second; its v axis runs across it. Only `Line`
    /// effects may use it (it binds the line), and only with the one `Point` placement.
    Ribbon {
        /// Texture element (its u axis along the strip).
        texture: String,
        /// Colour ramp.
        ramp: String,
        /// `Camera` turns the strip about its own axis to face the camera; `Ground` lays it
        /// flat. Other orientations are refused.
        #[serde(default)]
        orient: Orient,
        /// World units of strip per texture repeat, > 0.
        #[serde(default = "one")]
        repeat: f32,
        /// Texture repeats per second the texture scrolls along the strip, away from its
        /// start (negative: towards it).
        #[serde(default)]
        scroll: f32,
        /// Speed in world units per second at which the head races along the line; 0 draws
        /// the whole line from the start.
        #[serde(default)]
        travel: f32,
        /// Seconds after the layer's start at which the tail leaves the line's start and
        /// follows the head at `travel`; `None` keeps the tail at the start.
        #[serde(default)]
        tail_delay: Option<f32>,
        /// Share of the drawn length at each end over which the strip narrows to a point,
        /// `(at the tail, at the head)`, each in [0, 0.5].
        #[serde(default = "default_taper")]
        taper: (f32, f32),
        /// Height of the strip above the line, world units.
        #[serde(default)]
        height: f32,
        /// Soft-depth fade distance in world units; 0 for a hard intersection.
        #[serde(default)]
        soft_depth: f32,
        /// Erosion threshold over the layer's life.
        #[serde(default)]
        erosion: Option<String>,
        /// Moves the strip this many world units towards the camera in the sort only.
        #[serde(default)]
        sort_bias: f32,
        /// `Texture` draws the element tiled along the strip; `Radial` computes a soft core
        /// across it (a Gaussian across the width, densest on the axis), light rather than
        /// matter, whose texture is not read.
        #[serde(default)]
        shape: CardShape,
        /// Noise breakup of the erosion and edges, in the strip's own coordinates (it repeats
        /// and scrolls with the texture).
        #[serde(default)]
        breakup: Option<BreakupDef>,
    },
    /// A screen-space refraction (plan §5.1 "Distortion", WP7): heat haze over a burn, air
    /// rippling around a blast, a cloaked body's shimmer. It bends what is already drawn opaque
    /// behind it (the ground, unit bodies) by a scrolling noise inside a soft disc of the
    /// layer's `size` and adds no colour of its own. It is drawn in the transmissive phase from
    /// the view's copy of the opaque scene.
    Distortion {
        /// Noise element (a `NoiseV1` element); two decorrelated reads of it give the offset.
        noise: String,
        /// Noise repeats across the disc, > 0.
        #[serde(default = "one")]
        scale: f32,
        /// Noise scroll in repeats per second, `(u, v)` (v runs down the card: heat rising
        /// from the ground scrolls with a positive v on a camera-facing disc).
        #[serde(default)]
        scroll: (f32, f32),
        /// Largest offset of the scene behind, as a fraction of the disc's radius on screen,
        /// in (0, 0.25].
        strength: f32,
        /// `Camera` (a disc facing the camera) or `Ground` (a disc lying flat); other
        /// orientations are refused.
        #[serde(default)]
        orient: Orient,
        /// Height of the disc's centre above its placed point, world units.
        #[serde(default)]
        height: f32,
        /// Lens bend, in [0, 0.25]: the scene behind is pulled outward as a glass sphere
        /// refracts it, most strongly a little inside the rim, by up to this fraction of the
        /// disc's radius on screen; 0 for none. A cloaked body's outline is drawn this way
        /// (plan §5.5 `cloak_shimmer`), with no colour of its own.
        #[serde(default)]
        lens: f32,
    },
    /// A modulation of the unit's own bubble (plan §5.1 `Shell`, WP8). The effect renderer
    /// does not draw it: it is evaluated each frame for the unit its effect is held on and
    /// written into that bubble's material. Every value is its strength at the layer's
    /// intensity 1; the intensity (the layer's `curve` over its life) scales it towards no
    /// change. Only `Mode` and `Status` effects may have one.
    Shell {
        /// Share of the bubble's diameter sunk below where it rests, in [0, 1]; the ground
        /// hides what is below it (a burrowing unit).
        #[serde(default)]
        sink: f32,
        /// Linear RGB tint and the share of the media it replaces, each in [0, 1].
        #[serde(default)]
        tint: (f32, f32, f32, f32),
        /// Linear RGB of a light along the bubble's outline and its strength, each in [0, 1].
        #[serde(default)]
        rim: (f32, f32, f32, f32),
        /// Share of the bubble made see-through, in [0, 1]: its media fades and only the
        /// glass's outline and rim stay (a cloaked unit its owner sees).
        #[serde(default)]
        veil: f32,
        /// Height above the ground, as a share of the bubble's radius in [0, 1], over which the
        /// glass is stained by soil (a unit digging in).
        #[serde(default)]
        soil: f32,
        /// With `true` the layer's life is the progress of the mode switch its effect shows
        /// (`ChannelProgress`), not its age: the bubble sinks as the switch completes.
        #[serde(default)]
        progress: bool,
        /// The sink, in [0, 1], used instead of `sink` for a unit that shows its own clip for
        /// the mode (WP13 media): the clip itself shows the state, so the bubble sinks only a
        /// little and stays readable (WP8 review 1, B-1). `None` uses `sink` for every unit.
        #[serde(default)]
        clip_sink: Option<f32>,
        /// Share of the glass's specular highlight taken away, in [0, 1] (a highlight makes a
        /// sunk dome read as a whole ball).
        #[serde(default)]
        matte: f32,
        /// Share of the unit's shadow taken away, in [0, 1]: the shadow caster shrinks into
        /// the body as this rises, so the shadow fades with the look instead of popping.
        #[serde(default)]
        shadow: f32,
        /// A slow beat of the look's tint and rim (a stimmed body's racing pulse, plan §5.5
        /// `stim_surge`): see [`PulseDef`]. `None` holds them steady.
        #[serde(default)]
        pulse: Option<PulseDef>,
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
        /// With an event line, the particles are cut off where the line starts and ends.
        #[serde(default)]
        clip_to_line: bool,
    },
}

/// A beat of a `Shell` look's tint and rim: their strength is scaled by
/// `1 - depth * (0.5 - 0.5 * cos(2π · age / period))`, full at the beat's peak, `1 - depth`
/// between beats, a smooth swell with no hard step.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PulseDef {
    /// Seconds of one beat, positive.
    pub period: f32,
    /// Share of the strength the beat takes away between peaks, in [0, 1].
    pub depth: f32,
}

/// A factor on a layer's intensity from a bound value (plan WP9: a barrier's shell thins as it
/// absorbs): `floor + (1 - floor) · clamp(value, 0, 1)`, so the layer never vanishes while its
/// effect is shown.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrengthDef {
    /// The bound value, read as a fraction in [0, 1].
    pub bind: Binding,
    /// The factor at a bound value of 0, in [0, 1].
    #[serde(default)]
    pub floor: f32,
}

/// An effect a `Status` effect lays again and again while it is held (plan WP9): the dust a
/// charging unit kicks up, the fumes of a stimmed one. Each is a separate one-shot spawn at the
/// unit's foot, so it stays where it was laid while the unit moves on. An `Area` effect lays
/// another `Area` effect at the area's centre with the area's values (a storm's flares, WP10).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EmitDef {
    /// The effect laid; of its layer's own kind (`Status` or `Area`).
    pub effect: String,
    /// Seconds between two, positive.
    pub every: f32,
    /// Laid only while the unit moves (a `Status` effect only).
    #[serde(default)]
    pub moving: bool,
}

/// The effect a `Status` effect spawns on the unit when the simulation publishes a hit on its
/// status (a barrier absorbing damage, `StatusFlags::barrier_hit`; plan WP9 "absorb flash").
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HitDef {
    /// The effect spawned; a `Status` effect.
    pub effect: String,
    /// Least seconds between two while hits go on, positive: a barrier under steady fire
    /// flashes at this beat rather than once per frame.
    pub every: f32,
}

/// Serde default of a ribbon's taper: a tenth of its length at each end.
const fn default_taper() -> (f32, f32) {
    (0.1, 0.1)
}

/// Serde default of a decal's grazing cosine.
const fn default_grazing() -> f32 {
    0.35
}

/// The default clip feather: an anti-aliased edge, so a mark ends at its line's end within a
/// pixel at the default zoom.
const fn default_clip_feather() -> f32 {
    0.015
}

/// A decal drawn as a wet liquid (plan Amendment 2, the acid stain of review round 3).
///
/// The element's shading is read as the liquid's thickness. Pooling noise (the layer's
/// breakup noise, sampled at twice its scale) varies the value and the thickness, so
/// the fill pools darker and thinner in places instead of lying flat. The thickness field is
/// lit as a surface by a key light from the upper left of the screen: a diffuse term shades
/// its relief and a specular term draws a wet sheen. Both fade as the layer's ramp curve dries
/// the liquid: full at a ramp factor of 1, gone at `dry_below`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LiquidDef {
    /// Slope scale of the thickness field, > 0: higher is a more pronounced relief.
    pub relief: f32,
    /// Strength of the specular sheen at full wetness, in [0, 4].
    pub sheen: f32,
    /// Specular exponent, at least 1: higher is a tighter highlight.
    pub gloss: f32,
    /// Weight of the low-frequency noise in the value and the thickness, in [0, 1].
    pub value_noise: f32,
    /// Ramp factor at which the liquid counts as dry, in [0, 1).
    pub dry_below: f32,
}

/// A decal whose visible edge is a disc instead of its texture's outline (review round 5): a
/// `Critical` mark of an area (the acid stain) must end on the bound radius it marks, which a
/// texture's ragged outline and deep erosion bites cannot promise.
///
/// The disc's radius is the box's half length (an ellipse for `width` other than 1), less a
/// shallow noise: the edge wavers between `1 - 2 * edge_noise` and 1 of the half length, read
/// from the layer's breakup noise along the edge, so a mark sized `1 / (1 - edge_noise)` of a
/// radius has its mean edge on that radius. Inside the disc the texture is drawn from its
/// middle (`fill` of its half width across the disc's radius), so its own outline lies outside
/// the disc; its erosion order still erodes the mark from its edge and from holes as the
/// layer's erosion curve rises.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutlineDef {
    /// Depth of the edge's noise as a fraction of the half length, in [0, 0.25].
    pub edge_noise: f32,
    /// Share of the texture's half width read across the disc's radius, in [0.25, 1].
    pub fill: f32,
    /// Share of the half length, inside the edge, over which the mark fades out to it (as the
    /// square of the distance in from the edge, so its outermost part fades in slowly), in
    /// [0, 0.8]; 0 for an edge anti-aliased over a pixel (WP8 review 3, A3-1: soil pressed
    /// down by a hull fades into the floor round it).
    #[serde(default)]
    pub soft: f32,
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
    /// One instance at the far end of the event's line, moved back along the line towards its
    /// start by `back` world units (never past the start): a tether's grip set against its
    /// target's bubble at any range (WP10 review 3, A3-1 (b)). The effect's anchor is the
    /// line's start.
    LineEnd {
        /// Distance back from the line's far end.
        back: ValueDef,
    },
    /// `count` instances scattered in a disc of `radius`.
    Disc {
        /// Instances.
        count: u32,
        /// Disc radius.
        radius: ValueDef,
    },
    /// `count` instances round a circle of `radius`, evenly spaced from a start angle drawn per
    /// effect, each turned by up to a third of the spacing and moved in or out by up to
    /// `spread` of the radius (both hashed): a ring of heaped earth round a body, which a
    /// `Disc` would mostly scatter under it (WP8 review 2, B2-1).
    Circle {
        /// Instances.
        count: u32,
        /// Circle radius.
        radius: ValueDef,
        /// Largest move in or out, as a fraction of the radius, in [0, 0.5].
        #[serde(default)]
        spread: f32,
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
        /// Instances per point (a clump), at least 1.
        #[serde(default = "one_u32")]
        per_point: u32,
        /// Each instance is moved by a hashed offset of up to this many world units along
        /// and across the line.
        #[serde(default)]
        scatter: f32,
        /// Each instance's delay grows by a hashed `0..delay_jitter` seconds.
        #[serde(default)]
        delay_jitter: f32,
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
    /// Factor on `size` over the layer's life (cards, decals and lights; a ring's thickness);
    /// `None` is 1.
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
    /// The layer lives while the fact its effect shows holds (a unit's stance, its cloak, its
    /// status): it comes in over `duration` along its curves, then stays at their end until the
    /// effect is released, and erodes out over `exit` seconds (plan §5.4, WP8, WP9). Only `Mode`,
    /// `Status`, `Area` and `Link` effects may hold layers (an area or a tether is held while the
    /// simulation publishes it, WP10).
    #[serde(default)]
    pub held: bool,
    /// Seconds a held layer takes to erode out once its effect is released; positive.
    #[serde(default = "default_exit")]
    pub exit: f32,
    /// A factor on the layer's intensity from a bound value (see [`StrengthDef`]); `None` is 1.
    #[serde(default)]
    pub strength: Option<StrengthDef>,
}

/// Default seconds a held layer takes to erode out once released.
const fn default_exit() -> f32 {
    0.35
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
    /// Seconds between spawns while the fact it shows holds (a trail laid while a burrowed
    /// unit moves, plan Amendment 2, R5); positive. Required for an effect named in a mode's
    /// `moving` slot, unused elsewhere.
    #[serde(default)]
    pub repeat: Option<f32>,
    /// The salt of its instances' random choices (rotations, placements, variants, noise
    /// phases), when a review accepted the pictures of one draw and the effect must keep it;
    /// any `u32`. Without it the salt is the FNV-1a hash of the effect's name.
    #[serde(default)]
    pub seed: Option<u32>,
    /// For a mode switch's effect (a `Mode` effect named as `enter`, `leave` or `transition`):
    /// the seconds the morph takes on screen. When it is longer than the switch's channel the
    /// morph plays at its own pace and finishes after the channel has ended (the dig of a unit
    /// whose stats change before its animation ends, plan D-6, Q0069); without it, or when it
    /// is shorter, the morph follows the channel. Positive.
    #[serde(default)]
    pub span: Option<f32>,
    /// For a `Status` or `Area` effect: what it lays again and again while it is held (see
    /// [`EmitDef`]).
    #[serde(default)]
    pub emit: Option<EmitDef>,
    /// For a `Status` effect: what it spawns when the simulation publishes a hit on its status
    /// (see [`HitDef`]).
    #[serde(default)]
    pub hit: Option<HitDef>,
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
    /// Effect spawned where a particle falls back to the height it started at (a droplet
    /// leaving a splat); the particle ends there. The effect's event kind must be `Point`.
    #[serde(default)]
    pub land: Option<String>,
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
    /// Distortion discs per frame.
    #[serde(default = "default_distortions")]
    pub distortions: u32,
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

/// Serde default of `distortions`: the plan's §5.6 budget.
const fn default_distortions() -> u32 {
    64
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
    /// How concealed units look (plan §5.2 `defaults.concealment`, D-5).
    #[serde(default)]
    pub concealment: ConcealmentDefaultsDef,
    /// Mode switches and stances a unit's block names nothing for (plan §5.2 `defaults.mode`).
    #[serde(default)]
    pub mode: ModeDefaultsDef,
}

/// The looks of concealment; every entry is optional and names a `Mode` effect.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConcealmentDefaultsDef {
    /// Channel into a burrowed mode (a mode with `burrowed: true`).
    #[serde(default)]
    pub burrow_enter: Option<String>,
    /// Channel out of a burrowed mode.
    #[serde(default)]
    pub burrow_leave: Option<String>,
    /// Held while burrowed.
    #[serde(default)]
    pub burrowed: Option<String>,
    /// Held on a cloaked unit of the viewer's own (or of anyone, with no fog).
    #[serde(default)]
    pub cloaked_own: Option<String>,
    /// Held on a cloaked enemy the viewer does not detect: all the viewer sees of it.
    #[serde(default)]
    pub cloaked_enemy: Option<String>,
    /// Held on a concealed enemy the viewer detects.
    #[serde(default)]
    pub detected: Option<String>,
}

/// The looks of mode switches; every entry is optional and names a `Mode` effect.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeDefaultsDef {
    /// Either channel of a mode that anchors its unit (`is_immobile`) and is not burrowed.
    #[serde(default)]
    pub transition: Option<String>,
    /// Held while in such a mode.
    #[serde(default)]
    pub anchored: Option<String>,
}
