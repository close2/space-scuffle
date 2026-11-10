//! What the effect lab captures: the subject effect, the takes and their shots (plan §6.5).
//!
//! # Responsibility
//! Owns the declarative [`LabProgram`]: one subject effect with its bindings, and a list of
//! [`LabTake`]s, each a fresh spawn of the subject on one ground swatch under one camera zoom,
//! captured at a sorted list of ages. Also owns the pure scheduling rule
//! ([`next_step`]) and the standard programs: the eight contact-sheet ages and the 3-second
//! sequence at the showcase's 40 ms step ([`LabProgram::contact_sheet`]).
//!
//! # How it fits
//! The lab driver ([`super::driver`]) walks the program frame by frame; the effect-lab capture
//! tool (`client_diagnostics`) and the effect GPU tests build programs and read the PNGs it
//! writes.
//!
//! # Invariants
//! - Shot ages within a take are sorted and finite; a take's time step is in (0, 0.25] s
//!   (`Time<Virtual>`'s largest step), so particles integrate the same way on every run.
//! - Effect time advances only between captures; a capture is taken at its exact age.

use super::super::binding::BoundValues;
use super::super::catalog::schema::Priority;
use crate::camera::RtsCamera;
use bevy::math::{Vec2, Vec3};
use bevy::prelude::Resource;
use std::path::PathBuf;

/// The eight ages of the contact sheet, as fractions of the effect's lifetime (plan §6.5).
pub const CONTACT_AGES: [f32; 8] = [0.0, 0.03, 0.08, 0.15, 0.3, 0.5, 0.75, 1.0];

/// The showcase's frame step, seconds of `Time<Virtual>`.
pub const SEQUENCE_STEP: f32 = 0.04;

/// Length of the contact sheet's sequence, seconds.
pub const SEQUENCE_SECONDS: f32 = 3.0;

/// The largest time step a take may use (`Time<Virtual>`'s default maximum delta).
pub const MAX_STEP: f32 = 0.25;

/// Ages closer than this are the same age, seconds.
pub const AGE_TOLERANCE: f32 = 1e-4;

/// The ground swatch under the effect: the brightest or the darkest ground layer of the
/// theme, by the mean luminance of its albedo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabGround {
    /// The brightest ground layer.
    Bright,
    /// The darkest ground layer.
    Dark,
}

impl LabGround {
    /// Both swatches.
    pub const ALL: [Self; 2] = [Self::Bright, Self::Dark];

    /// Short name for file names.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Bright => "bright",
            Self::Dark => "dark",
        }
    }
}

/// The battle camera's zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LabZoom {
    /// The battle's default distance ([`RtsCamera::default`]).
    Default,
    /// The close hero framing ([`RtsCamera::hero_framing`]).
    Close,
}

impl LabZoom {
    /// Both zooms.
    pub const ALL: [Self; 2] = [Self::Default, Self::Close];

    /// Short name for file names.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Close => "close",
        }
    }

    /// The battle camera at this zoom, looking at `focus` on the ground (y = 0), at the
    /// battle's pitch.
    #[must_use]
    pub fn camera(self, focus: Vec2) -> RtsCamera {
        let mut camera = match self {
            Self::Default => RtsCamera::default(),
            Self::Close => RtsCamera::hero_framing(),
        };
        camera.target = focus;
        camera.look_at_y = 0.0;
        camera
    }
}

/// The effect the lab shows and the values it is bound to.
#[derive(Debug, Clone, PartialEq)]
pub struct LabSubject {
    /// Name of the effect in the vocabulary.
    pub effect: String,
    /// Bound values, as an event would publish them.
    pub bound: BoundValues,
    /// Anchor on the ground, world units (y is ignored: the ground is at 0).
    pub anchor: Vec3,
    /// End of the event's line, for a line effect; its start is the anchor.
    pub line_to: Option<Vec3>,
}

impl LabSubject {
    /// Where the camera looks: the anchor, or the middle of the line.
    #[must_use]
    pub fn focus(&self) -> Vec2 {
        let centre = self
            .line_to
            .map_or(self.anchor, |to| (self.anchor + to) * 0.5);
        Vec2::new(centre.x, centre.z)
    }
}

/// One capture of a take.
#[derive(Debug, Clone, PartialEq)]
pub struct LabShot {
    /// Seconds since the take's spawn.
    pub age: f32,
    /// Whether the 1-unit grid and the bound radius or line are drawn over it.
    pub overlay: bool,
    /// File stem of the PNG, written into the program's output directory.
    pub name: String,
}

/// One spawn of the subject on one swatch under one zoom, and its captures.
#[derive(Debug, Clone, PartialEq)]
pub struct LabTake {
    /// Ground swatch.
    pub ground: LabGround,
    /// Camera zoom.
    pub zoom: LabZoom,
    /// Whether the subject is spawned (false captures the empty ground for comparisons).
    pub effect: bool,
    /// Only layers of this priority are drawn, when set.
    pub only: Option<Priority>,
    /// Largest time step between captures, seconds.
    pub step: f32,
    /// Captures, sorted by age.
    pub shots: Vec<LabShot>,
}

/// A whole lab run.
#[derive(Resource, Debug, Clone, PartialEq)]
pub struct LabProgram {
    /// The effect shown.
    pub subject: LabSubject,
    /// Takes, in order.
    pub takes: Vec<LabTake>,
    /// Directory the PNGs are written into.
    pub out: PathBuf,
}

/// A program the lab cannot run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LabProgramError {
    /// A take's shots are not sorted by age, or an age is negative or not finite.
    #[error("take {take}: shot ages must be finite, non-negative and sorted")]
    UnsortedAges {
        /// Index of the take.
        take: usize,
    },
    /// A take's step is outside (0, `MAX_STEP`].
    #[error("take {take}: step {step} s is outside (0, {MAX_STEP}]")]
    Step {
        /// Index of the take.
        take: usize,
        /// The step.
        step: f32,
    },
}

impl LabProgram {
    /// Checks the program's invariants.
    ///
    /// # Errors
    /// [`LabProgramError`] naming the first take that breaks one.
    pub fn validate(&self) -> Result<(), LabProgramError> {
        for (index, take) in self.takes.iter().enumerate() {
            if !(take.step > 0.0 && take.step <= MAX_STEP) {
                return Err(LabProgramError::Step {
                    take: index,
                    step: take.step,
                });
            }
            let sorted = take.shots.windows(2).all(|pair| pair[0].age <= pair[1].age);
            let valid = take
                .shots
                .iter()
                .all(|shot| shot.age.is_finite() && shot.age >= 0.0);
            if !sorted || !valid {
                return Err(LabProgramError::UnsortedAges { take: index });
            }
        }
        Ok(())
    }

    /// The contact-sheet program for an effect living `lifetime` seconds: for each ground
    /// and zoom the eight [`CONTACT_AGES`], each plain and with the overlay, named
    /// `<ground>_<zoom>_<age index>[_ref]`; then the 3-second sequence on the bright ground
    /// at the default zoom, every [`SEQUENCE_STEP`], named `seq_<frame>`.
    #[must_use]
    pub fn contact_sheet(subject: LabSubject, lifetime: f32, out: PathBuf) -> Self {
        let mut takes = Vec::with_capacity(5);
        for ground in LabGround::ALL {
            for zoom in LabZoom::ALL {
                let mut shots = Vec::with_capacity(CONTACT_AGES.len() * 2);
                for (index, fraction) in CONTACT_AGES.iter().enumerate() {
                    let stem = format!("{}_{}_{index}", ground.label(), zoom.label());
                    let age = fraction * lifetime;
                    shots.push(LabShot {
                        age,
                        overlay: false,
                        name: stem.clone(),
                    });
                    shots.push(LabShot {
                        age,
                        overlay: true,
                        name: format!("{stem}_ref"),
                    });
                }
                takes.push(LabTake {
                    ground,
                    zoom,
                    effect: true,
                    only: None,
                    step: SEQUENCE_STEP,
                    shots,
                });
            }
        }
        takes.push(sequence_take(LabGround::Bright, LabZoom::Default, "seq"));
        Self {
            subject,
            takes,
            out,
        }
    }
}

/// The 3-second sequence at the 40 ms step, files `<prefix>_<frame:03>`.
#[must_use]
pub fn sequence_take(ground: LabGround, zoom: LabZoom, prefix: &str) -> LabTake {
    let frames = sequence_frames();
    LabTake {
        ground,
        zoom,
        effect: true,
        only: None,
        step: SEQUENCE_STEP,
        shots: (0..frames)
            .map(|frame| LabShot {
                age: frame_age(frame),
                overlay: false,
                name: format!("{prefix}_{frame:03}"),
            })
            .collect(),
    }
}

/// Frames of the sequence, its first at age 0 and its last at [`SEQUENCE_SECONDS`].
#[must_use]
pub fn sequence_frames() -> u32 {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a positive frame count far below u32::MAX"
    )]
    let frames = (SEQUENCE_SECONDS / SEQUENCE_STEP).round() as u32;
    frames + 1
}

/// Age of sequence frame `frame`, seconds.
#[must_use]
pub fn frame_age(frame: u32) -> f32 {
    #[expect(
        clippy::cast_precision_loss,
        reason = "frame indices are far below 2^24"
    )]
    let frame = frame as f32;
    frame * SEQUENCE_STEP
}

/// The time step that brings an effect at `age` towards `target` with steps of at most
/// `step`; 0 once `target` is reached (within [`AGE_TOLERANCE`]).
#[must_use]
pub fn next_step(age: f32, target: f32, step: f32) -> f32 {
    let remaining = target - age;
    if remaining <= AGE_TOLERANCE {
        return 0.0;
    }
    remaining.min(step)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn subject() -> LabSubject {
        LabSubject {
            effect: "any".into(),
            bound: BoundValues::default(),
            anchor: Vec3::new(-4.0, 0.0, 0.0),
            line_to: Some(Vec3::new(4.0, 0.0, 0.0)),
        }
    }

    #[test]
    fn steps_never_overshoot_a_capture_and_stop_on_it() {
        assert!((next_step(0.0, 0.1, 0.04) - 0.04).abs() < 1e-6);
        assert!((next_step(0.08, 0.1, 0.04) - 0.02).abs() < 1e-6);
        assert!(next_step(0.1, 0.1, 0.04).abs() < 1e-9);
        assert!(next_step(0.2, 0.1, 0.04).abs() < 1e-9, "past the target");
        let mut age = 0.0_f32;
        let mut steps = 0;
        while next_step(age, 0.3, 0.04) > 0.0 {
            age += next_step(age, 0.3, 0.04);
            steps += 1;
        }
        assert_eq!(steps, 8, "seven whole steps and one partial");
        assert!((age - 0.3).abs() < 1e-5);
    }

    #[test]
    fn the_contact_sheet_covers_every_age_ground_and_zoom_and_a_sequence() {
        let program = LabProgram::contact_sheet(subject(), 2.0, PathBuf::from("out"));
        assert_eq!(program.takes.len(), 5);
        assert_eq!(program.validate(), Ok(()), "the standard program is valid");
        for take in &program.takes[..4] {
            assert_eq!(take.shots.len(), 16);
            assert!(
                (take.shots[15].age - 2.0).abs() < 1e-6,
                "the last age is the lifetime"
            );
            assert_eq!(take.shots.iter().filter(|shot| shot.overlay).count(), 8);
        }
        let sequence = &program.takes[4];
        assert_eq!(sequence.shots.len(), 76, "0 to 3 s every 40 ms");
        assert!((sequence.shots[75].age - 3.0).abs() < 1e-5);
        assert_eq!(sequence.shots[1].name, "seq_001");
        assert_eq!(
            program.subject.focus(),
            Vec2::ZERO,
            "a line is framed at its middle"
        );
    }

    #[test]
    fn invalid_takes_are_refused() {
        let mut program = LabProgram::contact_sheet(subject(), 1.0, PathBuf::from("out"));
        program.takes[1].shots.swap(0, 4);
        assert_eq!(
            program.validate(),
            Err(LabProgramError::UnsortedAges { take: 1 })
        );
        let mut program = LabProgram::contact_sheet(subject(), 1.0, PathBuf::from("out"));
        program.takes[0].step = 1.0;
        assert!(matches!(
            program.validate(),
            Err(LabProgramError::Step { take: 0, .. })
        ));
    }

    #[test]
    fn zooms_are_the_battle_cameras_at_the_battle_pitch() {
        let default = LabZoom::Default.camera(Vec2::new(3.0, 4.0));
        let close = LabZoom::Close.camera(Vec2::ZERO);
        assert!((default.distance - RtsCamera::default().distance).abs() < 1e-6);
        assert!((close.distance - RtsCamera::hero_framing().distance).abs() < 1e-6);
        assert!((default.pitch - RtsCamera::default().pitch).abs() < 1e-6);
        assert_eq!(default.target, Vec2::new(3.0, 4.0));
        assert!(default.look_at_y.abs() < 1e-6, "the lab's ground is at 0");
    }
}
