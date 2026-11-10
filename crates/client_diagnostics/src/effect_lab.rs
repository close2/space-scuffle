//! The `effect_lab` mode of `battle_capture`: an effect's contact sheet from real GPU captures
//! (plan `2026-11-09-effect-visualization` §6.5).
//!
//! # Responsibility
//! Parses the effect and its bindings from the command line, runs `client::effects::lab` with
//! the contact-sheet program (eight ages on the brightest and the darkest ground at the
//! battle's default and close zoom, each plain and with the 1-unit grid and bound radii, and a
//! 3-second sequence at the showcase's 40 ms step), and assembles the PNGs into
//! `<effect>_contact.png` (ages) and `<effect>_sequence.png` (sequence) with a text legend.
//!
//! # How it fits
//! The capture is the production effect plugin and shaders on a lab stage, driven by the lab's
//! own clock (`Time<Virtual>` in exact steps), so a sheet is reproducible on one driver. The
//! effect GPU tests in `client` run the same lab code.
//!
//! # Invariants
//! - Bindings are named as the vocabulary names them (`SplashReach=1.25`); a line effect's
//!   line runs along +x through the focus with length `LineLength`.
//! - A sheet is written only after every capture of the program exists.

use crate::runtime_capture::CaptureError;
use bevy::prelude::*;
use client::effects::binding::BoundValues;
use client::effects::catalog::schema::{Binding, EventKind};
use client::effects::catalog::{EffectCatalog, EFFECTS_CATALOG_PATH};
use client::effects::lab::{
    lab_app, LabGround, LabProgram, LabSubject, LabZoom, CONTACT_AGES, SEQUENCE_STEP,
};
use client::ResolvedAssetsRoot;
use image::{imageops, RgbImage};
use std::path::{Path, PathBuf};

/// Capture size: the battle's reference resolution of the plan's §10.2.
pub const CAPTURE_SIZE: UVec2 = UVec2::new(1280, 720);
/// Width and height of one contact-sheet cell.
const CELL: (u32, u32) = (320, 180);
/// Width and height of one sequence thumbnail.
const THUMB: (u32, u32) = (240, 135);
/// Sequence thumbnails per row.
const THUMBS_PER_ROW: u32 = 10;

/// What the mode runs: the effect, its bindings, where the frames and the sheets go.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectLabOptions {
    /// Name of the effect in the shipped vocabulary.
    pub effect: String,
    /// `Binding=value` pairs.
    pub bindings: Vec<String>,
    /// New directory for the frames.
    pub out: PathBuf,
    /// Directory of the sheets (the frame directory when `None`).
    pub sheet_dir: Option<PathBuf>,
    /// The D-11 experiment: an HDR camera with a thresholded bloom
    /// ([`crate::hdr_bloom`]); the sheets are then named `<effect>_hdr_*`.
    pub hdr_bloom: bool,
    /// The fog take of a line ([`fog_subject`]): the first search point the viewer sees.
    pub fog_seen_from: Option<u32>,
}

/// Parses `Binding=value` pairs into bound values.
///
/// # Errors
/// [`CaptureError::InvalidScript`] naming the first malformed pair.
pub fn parse_bindings(pairs: &[String]) -> Result<BoundValues, CaptureError> {
    let mut bound = BoundValues::default();
    for pair in pairs {
        let invalid =
            || CaptureError::InvalidScript(format!("binding {pair:?}: expected NAME=VALUE"));
        let (name, value) = pair.split_once('=').ok_or_else(invalid)?;
        let binding: Binding = ron::from_str(name.trim()).map_err(|_| invalid())?;
        let value: f32 = value.trim().parse().map_err(|_| invalid())?;
        if !value.is_finite() || value < 0.0 {
            return Err(invalid());
        }
        bound.set(binding, value);
    }
    Ok(bound)
}

/// The lab subject: the effect at the origin, or a line of `LineLength` along +x centred on
/// the origin.
#[must_use]
pub fn subject(effect: &str, kind: EventKind, bound: BoundValues) -> LabSubject {
    let length = bound.get(Binding::LineLength);
    let (anchor, line_to) = if kind == EventKind::Line && length > 0.0 {
        (
            Vec3::new(-length * 0.5, 0.0, 0.0),
            Some(Vec3::new(length * 0.5, 0.0, 0.0)),
        )
    } else {
        (Vec3::ZERO, None)
    };
    LabSubject {
        effect: effect.to_owned(),
        bound,
        anchor,
        line_to,
        line_seen: u64::MAX,
        fog_below_x: None,
    }
}

/// The fog take of a line subject: the line moved half a spacing along itself so that its
/// search points sit at cell centres, as for units standing on them, the viewer seeing the
/// points from `seen_from` on, and the fog over every cell before the cell edge half a spacing
/// before that point (the attacker, one spacing before the first point, lies in it). A subject
/// without a line is returned as it is.
#[must_use]
pub fn fog_subject(mut subject: LabSubject, seen_from: u32) -> LabSubject {
    let Some(to) = subject.line_to else {
        return subject;
    };
    let spacing = subject.bound.get(Binding::LineSpacing).max(f32::EPSILON);
    let shift = Vec3::new(spacing * 0.5, 0.0, 0.0);
    subject.anchor += shift;
    subject.line_to = Some(to + shift);
    subject.line_seen = u64::MAX.checked_shl(seen_from.min(63)).unwrap_or(0);
    #[expect(
        clippy::cast_precision_loss,
        reason = "a small point index is exact in f32"
    )]
    let edge = subject.anchor.x + (seen_from as f32 - 0.5) * spacing;
    subject.fog_below_x = Some(edge);
    subject
}

/// Runs the mode.
///
/// # Errors
/// [`CaptureError`] when the effect is unknown, the run fails or a sheet cannot be written.
pub fn run_effect_lab(options: &EffectLabOptions) -> Result<AppExit, CaptureError> {
    let root = ResolvedAssetsRoot::resolve().map_err(client::ClientStartupError::from)?;
    let catalog = EffectCatalog::load(&root.as_path().join(EFFECTS_CATALOG_PATH), root.as_path())
        .map_err(|error| CaptureError::Verification(error.to_string()))?;
    let effect = catalog
        .effect_id(&options.effect)
        .and_then(|id| catalog.effect(id))
        .ok_or_else(|| {
            CaptureError::InvalidScript(format!("no effect named {:?}", options.effect))
        })?;
    let bound = parse_bindings(&options.bindings)?;
    let mut subject = subject(&options.effect, effect.event, bound);
    // The sheet's last column is the true end: a line's far points erupt later by its travel.
    let line = subject
        .line_to
        .map(|to| client::effects::placement::WorldLine::whole(subject.anchor, to));
    let lifetime = effect.lifetime + client::effects::runtime::max_line_delay(&effect.layers, line);
    if let Some(seen_from) = options.fog_seen_from {
        subject = fog_subject(subject, seen_from);
    }
    std::fs::create_dir(&options.out).map_err(|source| CaptureError::Io {
        path: options.out.clone(),
        source,
    })?;
    let program = LabProgram::contact_sheet(subject, lifetime, options.out.clone());
    let mut app = lab_app(root, program.clone(), None, CAPTURE_SIZE)
        .map_err(|error| CaptureError::Verification(error.to_string()))?;
    if options.hdr_bloom {
        app.add_plugins(crate::hdr_bloom::HdrBloomPlugin);
    }
    let exit = client::render_compatibility::run_render_app(&mut app);
    if !exit.is_success() {
        return Ok(exit);
    }
    let sheets = options.sheet_dir.as_deref().unwrap_or(&options.out);
    let look = if options.hdr_bloom {
        Look::HdrBloom
    } else if options.fog_seen_from.is_some() {
        Look::Fog
    } else {
        Look::Ldr
    };
    assemble(&program, &options.effect, lifetime, look, sheets)?;
    Ok(exit)
}

/// Reads frame `name` of the program.
fn frame(program: &LabProgram, name: &str) -> Result<RgbImage, CaptureError> {
    let path = program.out.join(format!("{name}.png"));
    image::open(&path)
        .map(|image| image.to_rgb8())
        .map_err(|source| CaptureError::MediaImage { path, source })
}

/// How much of a frame a sheet's cell shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Framing {
    /// The middle half (each way) of a default-zoom frame, the middle three quarters of a
    /// close frame: a splash, which sits in the middle.
    Centre,
    /// The middle half of a default-zoom frame and the whole close frame: a line, whose ends
    /// the close crop would cut off.
    Line,
}

/// The cell of `image` shown on a sheet as `framing` crops it, scaled to `size`.
fn cell(image: &RgbImage, zoom: LabZoom, framing: Framing, size: (u32, u32)) -> RgbImage {
    let (width, height) = image.dimensions();
    let (keep_w, keep_h) = match (zoom, framing) {
        (LabZoom::Default, _) => (width / 2, height / 2),
        (LabZoom::Close, Framing::Centre) => (width * 3 / 4, height * 3 / 4),
        (LabZoom::Close, Framing::Line) => (width, height),
    };
    let view = imageops::crop_imm(
        image,
        (width - keep_w) / 2,
        (height - keep_h) / 2,
        keep_w,
        keep_h,
    )
    .to_image();
    imageops::resize(&view, size.0, size.1, imageops::FilterType::Triangle)
}

/// The camera a sheet was captured with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// The shipped camera: no HDR target, no bloom.
    Ldr,
    /// The D-11 experiment ([`crate::hdr_bloom`]).
    HdrBloom,
    /// The shipped camera, with the line running out of the fog ([`fog_subject`]).
    Fog,
}

impl Look {
    /// Infix of the sheets' file names.
    const fn infix(self) -> &'static str {
        match self {
            Self::Ldr => "",
            Self::HdrBloom => "_hdr",
            Self::Fog => "_fog",
        }
    }

    /// The legend's description of the camera.
    const fn description(self) -> &'static str {
        match self {
            Self::Ldr => "shipped camera (no HDR, no bloom)",
            Self::HdrBloom => {
                "HDR camera with bloom above 1.0 (`--hdr-bloom`, plan D-11 experiment)"
            }
            Self::Fog => {
                "shipped camera; the line's first search points and the attacker behind them lie in \
                 the viewer's fog (`--fog-seen-from`), the fogged ground darkened to a quarter with a \
                 hard edge as the battle terrain draws it, and only the seen points admitted (plan \
                 Amendment 2, R6)"
            }
        }
    }
}

/// Writes `<effect>[_hdr]_contact.png`, `_sequence.png` and `_contact.txt`.
fn assemble(
    program: &LabProgram,
    effect: &str,
    lifetime: f32,
    look: Look,
    directory: &Path,
) -> Result<(), CaptureError> {
    let stem = format!("{effect}{}", look.infix());
    let framing = if program.subject.line_to.is_some() {
        Framing::Line
    } else {
        Framing::Centre
    };
    std::fs::create_dir_all(directory).map_err(|source| CaptureError::Io {
        path: directory.to_path_buf(),
        source,
    })?;
    let columns = u32::try_from(CONTACT_AGES.len()).unwrap_or(8);
    let rows = 8;
    let mut sheet = RgbImage::from_pixel(columns * CELL.0, rows * CELL.1, image::Rgb([24, 24, 28]));
    let mut row = 0;
    for ground in LabGround::ALL {
        for zoom in LabZoom::ALL {
            for overlay in [false, true] {
                for column in 0..columns {
                    let suffix = if overlay { "_ref" } else { "" };
                    let name = format!("{}_{}_{column}{suffix}", ground.label(), zoom.label());
                    let image = cell(&frame(program, &name)?, zoom, framing, CELL);
                    imageops::replace(
                        &mut sheet,
                        &image,
                        i64::from(column * CELL.0),
                        i64::from(row * CELL.1),
                    );
                }
                row += 1;
            }
        }
    }
    save(&sheet, &directory.join(format!("{stem}_contact.png")))?;
    let frames = client::effects::lab::program::sequence_frames();
    let sequence_rows = frames.div_ceil(THUMBS_PER_ROW);
    let mut strip = RgbImage::from_pixel(
        THUMBS_PER_ROW * THUMB.0,
        sequence_rows * THUMB.1,
        image::Rgb([24, 24, 28]),
    );
    for index in 0..frames {
        let image = cell(
            &frame(program, &format!("seq_{index:03}"))?,
            LabZoom::Default,
            framing,
            THUMB,
        );
        imageops::replace(
            &mut strip,
            &image,
            i64::from(index % THUMBS_PER_ROW * THUMB.0),
            i64::from(index / THUMBS_PER_ROW * THUMB.1),
        );
    }
    save(&strip, &directory.join(format!("{stem}_sequence.png")))?;
    let legend = legend(program, effect, lifetime, look);
    let path = directory.join(format!("{stem}_contact.txt"));
    std::fs::write(&path, legend).map_err(|source| CaptureError::Io { path, source })
}

/// Saves `image` as PNG at `path`.
fn save(image: &RgbImage, path: &Path) -> Result<(), CaptureError> {
    image
        .save(path)
        .map_err(|error| CaptureError::Readback(format!("{}: {error}", path.display())))
}

/// The text legend of the sheets.
fn legend(program: &LabProgram, effect: &str, lifetime: f32, look: Look) -> String {
    let stem = format!("{effect}{}", look.infix());
    let ages: Vec<String> = CONTACT_AGES
        .iter()
        .map(|fraction| format!("{fraction} ({:.3} s)", fraction * lifetime))
        .collect();
    let bound = &program.subject.bound;
    let bindings = [
        Binding::SplashFull,
        Binding::SplashMid,
        Binding::SplashReach,
        Binding::LineLength,
        Binding::LineSpacing,
    ]
    .iter()
    .filter(|binding| bound.get(**binding) > 0.0)
    .map(|binding| format!("{binding:?}={}", bound.get(*binding)))
    .collect::<Vec<_>>()
    .join(", ");
    format!(
        "Effect contact sheet of '{effect}' (plan 2026-11-09-effect-visualization §6.5), made by\n\
         `battle_capture --effect-lab {effect}` on the production effect plugin and shaders.\n\
         Bindings: {bindings}. Lifetime: {lifetime:.3} s. Capture {}x{}, battle camera at 58° pitch.\n\
         Camera: {}.\n\
         {stem}_contact.png columns: ages {}.\n\
         Rows: bright ground default zoom, the same with the 1-unit grid and the bound radii (red full\n\
         damage, yellow reach; for a line, the line and the full radius at each search point), bright\n\
         close zoom, its reference, dark default, its reference, dark close, its reference.\n\
         Default-zoom cells show the middle half of the frame, close-zoom cells its middle three quarters\n\
         (a line's close-zoom cells the whole frame).\n\
         {stem}_sequence.png: the first 3 s on the bright ground at the default zoom, one frame every\n\
         {} ms, read left to right, top to bottom.\n",
        CAPTURE_SIZE.x,
        CAPTURE_SIZE.y,
        look.description(),
        ages.join(", "),
        (SEQUENCE_STEP * 1000.0).round(),
    )
}

#[cfg(test)]
#[allow(clippy::expect_used, reason = "a failed test is a panic")]
mod tests {
    use super::*;

    #[test]
    fn bindings_parse_by_their_vocabulary_names_and_refuse_garbage() {
        let bound = parse_bindings(&["SplashReach=1.25".into(), " SplashFull = 0.47".into()])
            .expect("valid pairs");
        assert!((bound.get(Binding::SplashReach) - 1.25).abs() < 1e-6);
        assert!((bound.get(Binding::SplashFull) - 0.47).abs() < 1e-6);
        for bad in [
            "SplashReach",
            "Nonsense=1",
            "SplashReach=x",
            "SplashReach=-1",
        ] {
            assert!(parse_bindings(&[bad.into()]).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_line_subject_is_centred_on_the_focus_and_a_splash_sits_on_it() {
        let bound = BoundValues::default()
            .with(Binding::LineLength, 8.0)
            .with(Binding::LineSpacing, 1.0);
        let line = subject("any", EventKind::Line, bound);
        assert_eq!(line.anchor, Vec3::new(-4.0, 0.0, 0.0));
        assert_eq!(line.line_to, Some(Vec3::new(4.0, 0.0, 0.0)));
        assert_eq!(line.focus(), Vec2::ZERO);
        let splash = subject("any", EventKind::Splash, bound);
        assert_eq!(splash.anchor, Vec3::ZERO);
        assert!(splash.line_to.is_none());
    }

    #[test]
    fn cells_keep_the_middle_of_a_default_frame() {
        let mut image = RgbImage::new(1280, 720);
        image.put_pixel(640, 360, image::Rgb([255, 0, 0]));
        let default = cell(&image, LabZoom::Default, Framing::Centre, (640, 360));
        assert_eq!(default.get_pixel(320, 180).0[0], 255, "1:1 middle crop");
        let close = cell(&image, LabZoom::Close, Framing::Centre, (960, 540));
        assert_eq!(close.get_pixel(480, 270).0[0], 255);
        // A line's close cell keeps the whole frame, so its ends stay in view.
        image.put_pixel(4, 360, image::Rgb([0, 255, 0]));
        let line = cell(&image, LabZoom::Close, Framing::Line, (1280, 720));
        assert_eq!(line.get_pixel(4, 360).0[1], 255);
    }
}
