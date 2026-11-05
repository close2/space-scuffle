//! The storyboard of the showcase video: takes, scenes, the teaser cut and the encoding budgets.
//!
//! # Responsibility
//! Owns the schema of `assets/captures/showcase.ron` (plan `docs/plans/2026-11-05-ci-showcase-video.md`,
//! decision D-4), its loading, and its validation. A *take* is one client process on one scenario; a
//! *scene* is a span of frames of a take with one camera move, one battle speed and one overlay.
//!
//! # How it fits
//! The `take` subcommand renders one take scene by scene ([`super::director`]); `assemble` cuts the
//! published files from the takes with [`Teaser`], [`Poster`] and [`VideoEncoding`].
//!
//! # Invariants
//! - [`Storyboard::validate`] reports the first fault in a fixed order, so a storyboard has exactly
//!   one reported fault however many it has (the same contract as `CaptureScript::validate`).
//! - Unit identities appear only in the RON file, never in Rust (check C5). Captions name the
//!   `DESIGN.md` §4 rows they show, and validation refuses a row that is missing, `Not built` or
//!   `Removed`, so a caption cannot advertise what the game does not have.
//! - The video frame rate equals the simulation tick rate, so a frame never shows a fractional tick.

use super::cards::{SubsystemRow, SubsystemStatus};
use super::ShowcaseError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The only supported storyboard schema version.
pub const FORMAT_VERSION: u32 = 1;

/// Longest scene, in frames (two minutes at 25 fps).
pub const MAX_SCENE_FRAMES: u32 = 3_000;

/// Most frames of all takes together, counting each scene at its maximum length.
pub const MAX_TOTAL_FRAMES: u32 = 4_000;

/// Allowed frame width in pixels.
pub const WIDTH_RANGE: std::ops::RangeInclusive<u32> = 320..=4_096;

/// Allowed frame height in pixels: 180 is the height of a 320-pixel-wide 16:9 frame.
pub const HEIGHT_RANGE: std::ops::RangeInclusive<u32> = 180..=4_096;

/// Allowed camera pitch, in degrees above the horizontal.
pub const PITCH_RANGE_DEG: std::ops::RangeInclusive<f32> = 15.0..=85.0;

/// Fastest battle speed a scene may request (the client's own maximum).
pub const MAX_SPEED: u32 = 4;

/// Longest take or scene name, in bytes.
const MAX_NAME_BYTES: usize = 32;

/// The whole storyboard file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Storyboard {
    /// Schema version; must be [`FORMAT_VERSION`].
    pub format_version: u32,
    /// Rendered frame size in physical pixels, `(width, height)`; both even.
    pub resolution: (u32, u32),
    /// Video frames per second; must equal `sim_types::TICKS_PER_SECOND`.
    pub frame_rate: u32,
    /// The takes, rendered and concatenated in order.
    pub takes: Vec<Take>,
    /// The full-quality MP4.
    pub video: VideoEncoding,
    /// The animated WebP teaser and its cut.
    pub teaser: Teaser,
    /// The still used as the video poster.
    pub poster: Poster,
}

/// One client process, one mode, one scenario, one continuous simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Take {
    /// Safe name: the mezzanine is `<name>.mkv`.
    pub name: String,
    /// The client mode the take runs in.
    pub mode: TakeMode,
    /// Scenario path relative to the assets root.
    pub scenario: PathBuf,
    /// Wall-clock seconds allowed for start-up: shaders, media and the first battle tick.
    pub readiness_timeout_seconds: u64,
    /// Scenes, in order.
    pub scenes: Vec<Scene>,
}

/// Client mode of a take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TakeMode {
    /// A battle under the Player profile: real HUD, real fog, no inspector.
    Skirmish,
    /// The developer sandbox: inspector panel, no fog.
    LiveBattle,
    /// Reserved for the Visual Lab; rejected in version 1 (its inspector cannot be hidden yet).
    VisualLab,
}

impl TakeMode {
    /// The production client mode, or `None` for a reserved mode.
    #[must_use]
    pub const fn client_mode(self) -> Option<client::ClientMode> {
        match self {
            Self::Skirmish => Some(client::ClientMode::Skirmish),
            Self::LiveBattle => Some(client::ClientMode::LiveBattle),
            Self::VisualLab => None,
        }
    }
}

/// A span of frames with one camera move, one speed and one overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Safe name, unique in the storyboard.
    pub name: String,
    /// Battle speed during the scene.
    pub speed: Speed,
    /// How long the scene lasts.
    pub length: Length,
    /// The camera move.
    pub camera: CameraMove,
    /// The text drawn over the scene.
    pub overlay: Overlay,
    /// Corner badge; required when the speed is above 1 so a time-lapse is never hidden.
    #[serde(default)]
    pub badge: Option<String>,
    /// Starts at the scene's own first pose instead of blending from the previous scene.
    #[serde(default)]
    pub cut: bool,
}

/// Battle speed of a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speed {
    /// The battle does not advance; the smog keeps moving.
    Paused,
    /// Ticks per 40 ms frame, `1..=MAX_SPEED`.
    Speed(u32),
}

/// Length of a scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Length {
    /// Exactly this many frames.
    Frames(u32),
    /// Until the first frame whose ticks contain hostile damage, but at least `min` frames; fails
    /// at `max` frames without combat.
    UntilCombat {
        /// Fewest frames.
        min: u32,
        /// Most frames; reaching it without combat fails the take.
        max: u32,
    },
}

impl Length {
    /// The fewest frames the scene can have.
    #[must_use]
    pub const fn min_frames(self) -> u32 {
        match self {
            Self::Frames(frames) => frames,
            Self::UntilCombat { min, .. } => min,
        }
    }

    /// The most frames the scene can have.
    #[must_use]
    pub const fn max_frames(self) -> u32 {
        match self {
            Self::Frames(frames) => frames,
            Self::UntilCombat { max, .. } => max,
        }
    }
}

/// A point on the map plane in world units, `(x, z)`.
pub type MapPoint = (f32, f32);

/// The camera move of a scene. Angles are degrees in the file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CameraMove {
    /// A still camera.
    Fixed {
        /// Ground focus.
        focus: MapPoint,
        /// Distance from the focus, world units.
        distance: f32,
        /// Pitch, degrees.
        pitch_deg: f32,
    },
    /// Smoothstep between keys at scene frames.
    Path {
        /// Keys sorted by frame; the first at frame 0.
        keys: Vec<PathKey>,
    },
    /// An orbit about a point, sweeping the yaw.
    Orbit {
        /// What the camera orbits.
        center: OrbitCenter,
        /// Distance from the centre, world units.
        distance: f32,
        /// Pitch, degrees.
        pitch_deg: f32,
        /// Yaw at the first frame, degrees; 0 looks north like the production camera.
        start_deg: f32,
        /// Yaw swept over the scene, degrees.
        sweep_deg: f32,
    },
    /// The centroid of a player's living mobile units.
    FollowArmy {
        /// Player slot.
        player: u8,
        /// Distance, world units.
        distance: f32,
        /// Pitch, degrees.
        pitch_deg: f32,
    },
    /// The centroid of the fight nearest the camera (victims of hostile damage in the last second
    /// of game time within 10 world units of the one nearest the previous focus), or the
    /// midpoint of the first two armies when there was none.
    FollowCombat {
        /// Distance, world units.
        distance: f32,
        /// Pitch, degrees.
        pitch_deg: f32,
    },
    /// The living unit with the lowest identifier of the first listed type that has one.
    FollowUnit {
        /// Player slot.
        player: u8,
        /// Catalog slugs in order of preference.
        unit_types: Vec<String>,
        /// Distance, world units.
        distance: f32,
        /// Pitch, degrees.
        pitch_deg: f32,
    },
}

/// One key of a [`CameraMove::Path`].
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PathKey {
    /// Scene frame the key is reached at.
    pub frame: u32,
    /// Ground focus.
    pub focus: MapPoint,
    /// Distance, world units.
    pub distance: f32,
    /// Pitch, degrees.
    pub pitch_deg: f32,
    /// Yaw, degrees.
    #[serde(default)]
    pub yaw_deg: f32,
}

/// The centre of an orbit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OrbitCenter {
    /// A fixed map point.
    At(MapPoint),
    /// Where the camera looked at the end of the previous scene.
    Current,
}

/// The text drawn over a scene.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overlay {
    /// Nothing.
    None,
    /// Project name, pitch and the revision footer, generated (see [`super::cards`]).
    TitleCard,
    /// A short hand-written caption.
    Caption {
        /// One line, about 28 points.
        title: String,
        /// At most two lines, about 18 points.
        body: String,
        /// The `DESIGN.md` §4 rows the scene shows.
        subsystems: Vec<String>,
    },
    /// "Where it is" and "Where it is heading", generated.
    StatusCard,
}

/// The MP4 encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoEncoding {
    /// x264 constant rate factor.
    pub crf: u32,
    /// Peak bit rate, kbit/s (`-maxrate`; the buffer is twice this).
    pub max_kbit_per_second: u32,
    /// Largest accepted file, bytes.
    pub max_bytes: u64,
}

/// The animated WebP teaser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Teaser {
    /// Keep every `frame_step`-th frame of the cut.
    pub frame_step: u32,
    /// Widths to try, largest first; the height keeps the aspect ratio.
    pub widths: Vec<u32>,
    /// WebP qualities to try at each width, highest first.
    pub qualities: Vec<u32>,
    /// Largest accepted file, bytes.
    pub max_bytes: u64,
    /// The spans of scenes, in order.
    pub cuts: Vec<Cut>,
}

/// A span of one scene in the teaser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cut {
    /// Scene name.
    pub scene: String,
    /// First frame of the scene, or frames before its end when `from_end`.
    #[serde(default)]
    pub first: u32,
    /// Frames in the cut.
    pub frames: u32,
    /// Count `first` back from the end: the cut ends `first` frames before the scene's end.
    #[serde(default)]
    pub from_end: bool,
}

/// The poster still.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Poster {
    /// Scene name.
    pub scene: String,
    /// Frame within that scene.
    pub frame: u32,
}

/// The first fault [`Storyboard::validate`] finds.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StoryboardFault {
    /// The schema version is not [`FORMAT_VERSION`].
    #[error("format_version {0} is not 1")]
    Version(u32),
    /// A side is outside [`WIDTH_RANGE`] or [`HEIGHT_RANGE`], or odd.
    #[error("resolution {0}x{1} must be even, 320..=4096 wide and 180..=4096 high")]
    Resolution(u32, u32),
    /// The frame rate differs from the tick rate.
    #[error("frame_rate {0} must equal the simulation tick rate")]
    FrameRate(u32),
    /// A name is empty, too long or has characters other than `a-z`, `0-9`, `_`, `-`.
    #[error("unsafe name `{0}`")]
    UnsafeName(String),
    /// Two takes or two scenes share a name.
    #[error("duplicate name `{0}`")]
    DuplicateName(String),
    /// There is no take, or a take has no scene.
    #[error("the storyboard needs at least one take with at least one scene")]
    NoScenes,
    /// A take uses a reserved mode.
    #[error("take `{0}` uses a mode version 1 does not support")]
    UnsupportedMode(String),
    /// A scene length is 0 or above [`MAX_SCENE_FRAMES`], or a path key lies outside the scene.
    #[error("scene `{0}` has a length outside 1..=3000 or a path key outside it")]
    Length(String),
    /// An `UntilCombat` scene has `min > max`.
    #[error("scene `{0}` has UntilCombat min above max")]
    UntilCombatOrder(String),
    /// All scenes at their maximum lengths exceed [`MAX_TOTAL_FRAMES`].
    #[error("the takes may run {0} frames, more than 4000")]
    TotalFrames(u32),
    /// A camera distance is outside the production camera's range.
    #[error("scene `{0}` has a camera distance outside the production range")]
    Distance(String),
    /// A pitch is outside [`PITCH_RANGE_DEG`].
    #[error("scene `{0}` has a pitch outside 15..=85 degrees")]
    Pitch(String),
    /// A number is NaN or infinite.
    #[error("scene `{0}` has a number that is not finite")]
    NotFinite(String),
    /// A teaser cut or the poster lies outside its scene's minimum length, or names no scene.
    #[error("teaser cut or poster on scene `{0}` lies outside the scene's minimum length")]
    Cut(String),
    /// The teaser or video encoding has an empty ladder or a zero budget or step.
    #[error("encoding settings are empty or zero: {0}")]
    Encoding(&'static str),
    /// A speed is 0 or above [`MAX_SPEED`].
    #[error("scene `{0}` has a speed outside 1..=4")]
    Speed(String),
    /// A speed above 1 has no badge.
    #[error("scene `{0}` runs faster than 1x without a badge")]
    MissingBadge(String),
    /// A caption names no row of `DESIGN.md` §4.
    #[error(
        "scene `{scene}` names subsystem `{subsystem}`, which DESIGN.md section 4 does not list"
    )]
    UnknownSubsystem {
        /// Scene.
        scene: String,
        /// The unknown name.
        subsystem: String,
    },
    /// A caption names a row that is `Not built` or `Removed`.
    #[error("scene `{scene}` names subsystem `{subsystem}`, whose status is {status:?}")]
    HiddenSubsystem {
        /// Scene.
        scene: String,
        /// The row.
        subsystem: String,
        /// Its status.
        status: SubsystemStatus,
    },
    /// A followed unit slug is not a catalog identity.
    #[error("scene `{scene}` follows `{slug}`, which is not in the unit catalog")]
    UnknownUnit {
        /// Scene.
        scene: String,
        /// The slug.
        slug: String,
    },
    /// A followed unit has no admitted media.
    #[error("scene `{scene}` follows `{slug}`, which has no admitted media: {reason}")]
    UnitWithoutMedia {
        /// Scene.
        scene: String,
        /// The slug.
        slug: String,
        /// The media loader's error.
        reason: String,
    },
}

/// What validation checks the storyboard against: the `DESIGN.md` rows, the catalog and the media.
pub struct ValidationContext<'a> {
    /// `DESIGN.md` §4 rows.
    pub subsystems: &'a [SubsystemRow],
    /// The unit catalog read from the assets root.
    pub catalog: &'a sim_core::catalog::UnitCatalog,
    /// The texture directory (`<assets>/textures`).
    pub textures: &'a Path,
}

impl Storyboard {
    /// Reads and parses a storyboard; it is not validated yet.
    ///
    /// # Errors
    /// [`ShowcaseError::Io`] or [`ShowcaseError::Parse`].
    pub fn load(path: &Path) -> Result<Self, ShowcaseError> {
        let text = std::fs::read_to_string(path).map_err(|source| ShowcaseError::Io {
            path: path.to_owned(),
            source,
        })?;
        ron::from_str(&text).map_err(|source| ShowcaseError::Parse {
            path: path.to_owned(),
            source,
        })
    }

    /// The take named `name`.
    #[must_use]
    pub fn take(&self, name: &str) -> Option<&Take> {
        self.takes.iter().find(|take| take.name == name)
    }

    /// The scene named `name`, with its take.
    #[must_use]
    pub fn scene(&self, name: &str) -> Option<(&Take, &Scene)> {
        self.takes.iter().find_map(|take| {
            take.scenes
                .iter()
                .find(|scene| scene.name == name)
                .map(|scene| (take, scene))
        })
    }

    /// Checks every rule of plan decision D-4 and returns the first fault.
    ///
    /// # Errors
    /// The first [`StoryboardFault`], in the order the rules are listed in D-4.
    pub fn validate(&self, context: &ValidationContext<'_>) -> Result<(), StoryboardFault> {
        self.validate_frame()?;
        self.validate_names()?;
        self.validate_lengths()?;
        self.validate_cameras()?;
        self.validate_publishing()?;
        self.validate_speeds()?;
        self.validate_captions(context.subsystems)?;
        self.validate_units(context)
    }

    fn scenes(&self) -> impl Iterator<Item = &Scene> {
        self.takes.iter().flat_map(|take| take.scenes.iter())
    }

    fn validate_frame(&self) -> Result<(), StoryboardFault> {
        if self.format_version != FORMAT_VERSION {
            return Err(StoryboardFault::Version(self.format_version));
        }
        let (width, height) = self.resolution;
        if !WIDTH_RANGE.contains(&width)
            || !HEIGHT_RANGE.contains(&height)
            || width % 2 != 0
            || height % 2 != 0
        {
            return Err(StoryboardFault::Resolution(width, height));
        }
        if self.frame_rate != u32::from(sim_types::TICKS_PER_SECOND) {
            return Err(StoryboardFault::FrameRate(self.frame_rate));
        }
        Ok(())
    }

    fn validate_names(&self) -> Result<(), StoryboardFault> {
        let mut takes = BTreeSet::new();
        let mut scenes = BTreeSet::new();
        for take in &self.takes {
            check_name(&take.name)?;
            if !takes.insert(take.name.as_str()) {
                return Err(StoryboardFault::DuplicateName(take.name.clone()));
            }
            for scene in &take.scenes {
                check_name(&scene.name)?;
                if !scenes.insert(scene.name.as_str()) {
                    return Err(StoryboardFault::DuplicateName(scene.name.clone()));
                }
            }
        }
        if self.takes.is_empty() || self.takes.iter().any(|take| take.scenes.is_empty()) {
            return Err(StoryboardFault::NoScenes);
        }
        if let Some(take) = self
            .takes
            .iter()
            .find(|take| take.mode.client_mode().is_none())
        {
            return Err(StoryboardFault::UnsupportedMode(take.name.clone()));
        }
        Ok(())
    }

    fn validate_lengths(&self) -> Result<(), StoryboardFault> {
        for scene in self.scenes() {
            let (min, max) = (scene.length.min_frames(), scene.length.max_frames());
            if min == 0 || max == 0 || min > MAX_SCENE_FRAMES || max > MAX_SCENE_FRAMES {
                return Err(StoryboardFault::Length(scene.name.clone()));
            }
            if let CameraMove::Path { keys } = &scene.camera {
                let sorted = keys.windows(2).all(|pair| pair[0].frame < pair[1].frame);
                let inside = keys.first().is_some_and(|key| key.frame == 0)
                    && keys.last().is_some_and(|key| key.frame < max);
                if !sorted || !inside {
                    return Err(StoryboardFault::Length(scene.name.clone()));
                }
            }
        }
        for scene in self.scenes() {
            if scene.length.min_frames() > scene.length.max_frames() {
                return Err(StoryboardFault::UntilCombatOrder(scene.name.clone()));
            }
        }
        let total: u32 = self
            .scenes()
            .map(|scene| scene.length.max_frames())
            .fold(0, u32::saturating_add);
        if total > MAX_TOTAL_FRAMES {
            return Err(StoryboardFault::TotalFrames(total));
        }
        Ok(())
    }

    fn validate_cameras(&self) -> Result<(), StoryboardFault> {
        let distances = client::MIN_CAMERA_DISTANCE..=client::MAX_CAMERA_DISTANCE;
        for scene in self.scenes() {
            let numbers = camera_numbers(&scene.camera);
            if numbers
                .distances
                .iter()
                .any(|value| !distances.contains(value))
            {
                return Err(StoryboardFault::Distance(scene.name.clone()));
            }
            if numbers
                .pitches
                .iter()
                .any(|value| !PITCH_RANGE_DEG.contains(value))
            {
                return Err(StoryboardFault::Pitch(scene.name.clone()));
            }
        }
        for scene in self.scenes() {
            let numbers = camera_numbers(&scene.camera);
            if !numbers.others.iter().all(|value| value.is_finite()) {
                return Err(StoryboardFault::NotFinite(scene.name.clone()));
            }
        }
        Ok(())
    }

    fn validate_publishing(&self) -> Result<(), StoryboardFault> {
        let inside = |scene: &str, first: u32, frames: u32| {
            self.scene(scene).is_some_and(|(_, found)| {
                frames > 0 && first.saturating_add(frames) <= found.length.min_frames()
            })
        };
        for cut in &self.teaser.cuts {
            if !inside(&cut.scene, cut.first, cut.frames) {
                return Err(StoryboardFault::Cut(cut.scene.clone()));
            }
        }
        if !inside(&self.poster.scene, self.poster.frame, 1) {
            return Err(StoryboardFault::Cut(self.poster.scene.clone()));
        }
        if self.teaser.cuts.is_empty() {
            return Err(StoryboardFault::Encoding("the teaser has no cut"));
        }
        if self.teaser.frame_step == 0
            || self.teaser.max_bytes == 0
            || self.video.max_bytes == 0
            || self.video.max_kbit_per_second == 0
        {
            return Err(StoryboardFault::Encoding("a step, rate or budget is zero"));
        }
        if self.teaser.widths.is_empty()
            || self.teaser.qualities.is_empty()
            || self
                .teaser
                .widths
                .iter()
                .any(|width| *width < 2 || width % 2 != 0)
            || self.teaser.qualities.iter().any(|quality| *quality > 100)
        {
            return Err(StoryboardFault::Encoding(
                "teaser widths must be even and qualities within 0..=100, neither list empty",
            ));
        }
        Ok(())
    }

    fn validate_speeds(&self) -> Result<(), StoryboardFault> {
        for scene in self.scenes() {
            if let Speed::Speed(speed) = scene.speed {
                if speed == 0 || speed > MAX_SPEED {
                    return Err(StoryboardFault::Speed(scene.name.clone()));
                }
                if speed > 1
                    && scene
                        .badge
                        .as_deref()
                        .is_none_or(|badge| badge.trim().is_empty())
                {
                    return Err(StoryboardFault::MissingBadge(scene.name.clone()));
                }
            }
        }
        Ok(())
    }

    fn validate_captions(&self, rows: &[SubsystemRow]) -> Result<(), StoryboardFault> {
        for scene in self.scenes() {
            let Overlay::Caption { subsystems, .. } = &scene.overlay else {
                continue;
            };
            for subsystem in subsystems {
                let row = rows
                    .iter()
                    .find(|row| &row.name == subsystem)
                    .ok_or_else(|| StoryboardFault::UnknownSubsystem {
                        scene: scene.name.clone(),
                        subsystem: subsystem.clone(),
                    })?;
                if !row.status.may_be_shown() {
                    return Err(StoryboardFault::HiddenSubsystem {
                        scene: scene.name.clone(),
                        subsystem: subsystem.clone(),
                        status: row.status,
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_units(&self, context: &ValidationContext<'_>) -> Result<(), StoryboardFault> {
        for scene in self.scenes() {
            let CameraMove::FollowUnit { unit_types, .. } = &scene.camera else {
                continue;
            };
            if unit_types.is_empty() {
                return Err(StoryboardFault::UnknownUnit {
                    scene: scene.name.clone(),
                    slug: String::new(),
                });
            }
            for slug in unit_types {
                if context.catalog.get_by_slug(slug).is_none() {
                    return Err(StoryboardFault::UnknownUnit {
                        scene: scene.name.clone(),
                        slug: slug.clone(),
                    });
                }
                asset_baker::media::load_unit_media(context.textures, slug, None).map_err(
                    |error| StoryboardFault::UnitWithoutMedia {
                        scene: scene.name.clone(),
                        slug: slug.clone(),
                        reason: error.to_string(),
                    },
                )?;
            }
        }
        Ok(())
    }
}

fn check_name(name: &str) -> Result<(), StoryboardFault> {
    if name.is_empty()
        || name.len() > MAX_NAME_BYTES
        || !name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(StoryboardFault::UnsafeName(name.to_owned()));
    }
    Ok(())
}

/// The numbers of a camera move, grouped by the rule that checks them.
struct CameraNumbers {
    distances: Vec<f32>,
    pitches: Vec<f32>,
    others: Vec<f32>,
}

fn camera_numbers(camera: &CameraMove) -> CameraNumbers {
    let mut numbers = CameraNumbers {
        distances: Vec::new(),
        pitches: Vec::new(),
        others: Vec::new(),
    };
    match camera {
        CameraMove::Fixed {
            focus,
            distance,
            pitch_deg,
        } => {
            numbers.distances.push(*distance);
            numbers.pitches.push(*pitch_deg);
            numbers.others.extend([focus.0, focus.1]);
        }
        CameraMove::Path { keys } => {
            for key in keys {
                numbers.distances.push(key.distance);
                numbers.pitches.push(key.pitch_deg);
                numbers
                    .others
                    .extend([key.focus.0, key.focus.1, key.yaw_deg]);
            }
        }
        CameraMove::Orbit {
            center,
            distance,
            pitch_deg,
            start_deg,
            sweep_deg,
        } => {
            numbers.distances.push(*distance);
            numbers.pitches.push(*pitch_deg);
            numbers.others.extend([*start_deg, *sweep_deg]);
            if let OrbitCenter::At(point) = center {
                numbers.others.extend([point.0, point.1]);
            }
        }
        CameraMove::FollowArmy {
            distance,
            pitch_deg,
            ..
        }
        | CameraMove::FollowCombat {
            distance,
            pitch_deg,
        }
        | CameraMove::FollowUnit {
            distance,
            pitch_deg,
            ..
        } => {
            numbers.distances.push(*distance);
            numbers.pitches.push(*pitch_deg);
        }
    }
    numbers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_allow_only_lowercase_digits_underscore_and_hyphen() {
        assert!(check_name("clash_2-a").is_ok());
        for bad in ["", "Clash", "a b", "../x", &"x".repeat(33)] {
            assert_eq!(
                check_name(bad),
                Err(StoryboardFault::UnsafeName(bad.to_owned()))
            );
        }
    }

    #[test]
    fn lengths_report_minimum_and_maximum() {
        assert_eq!(Length::Frames(7).min_frames(), 7);
        let until = Length::UntilCombat { min: 3, max: 9 };
        assert_eq!((until.min_frames(), until.max_frames()), (3, 9));
    }
}
