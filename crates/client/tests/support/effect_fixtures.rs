//! Test-only effect texture sets and vocabularies, written into a temporary directory.
//!
//! # Responsibility
//! Writes a small `PackedV1` texture set (`fx_fixture`) in the format `asset_baker` defines
//! (`asset_baker::effects::texture_set`) and effect vocabularies that reference it by absolute
//! path. Charter §1.2: procedural textures exist only as temporary test fixtures, never under
//! `assets/`; every path here is inside a `tempfile::TempDir`.
//!
//! # Elements of `fx_fixture` (cells of [`CELL`] pixels; R shading, G density, B erosion, A 0)
//! - `soft_disc`: a disc of radius 0.45 cell, density falling to 0 at its edge, shading 1 at the
//!   centre, erosion order rising from edge (0) to centre (1).
//! - `solid`: every texel shading 1, density 1, erosion 1.
//! - `flip`: four frames, frame `i` shading `(i + 1) / 4`, density 1.
//! - `data_sheet`: a data element (blend `Data`), which no block may draw.
//!
//! # Invariants
//! - Never writes outside the directory it is given.

#![allow(dead_code, clippy::expect_used, clippy::panic)]

use asset_baker::effects::texture_set::{
    mip_file_name, to_ron_text, ChannelLayout, EffectElement, EffectTextureSet, ElementBlend,
    EFFECT_FORMAT_VERSION,
};
use std::path::{Path, PathBuf};

/// Side of a level-0 cell in pixels.
pub const CELL: u32 = 32;
/// Mip levels of the fixture set.
pub const MIPS: u32 = 3;
/// Name of the fixture set.
pub const SET: &str = "fx_fixture";

/// One element of the fixture: name, frames, blend and the texel function of frame `f` at
/// cell coordinates `(u, v)` in 0..1.
struct FixtureElement {
    name: &'static str,
    frames: u32,
    blend: ElementBlend,
    texel: fn(u32, f32, f32) -> [u8; 4],
}

/// Converts 0..1 to a byte.
fn byte(value: f32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let level = (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    level
}

fn soft_disc(_: u32, u: f32, v: f32) -> [u8; 4] {
    let r = ((u - 0.5).powi(2) + (v - 0.5).powi(2)).sqrt() / 0.45;
    let inside = (1.0 - r).clamp(0.0, 1.0);
    [byte(1.0), byte((inside * 2.0).min(1.0)), byte(inside), 0]
}

fn solid(_: u32, _: f32, _: f32) -> [u8; 4] {
    [255, 255, 255, 0]
}

#[allow(clippy::cast_precision_loss)]
fn flip(frame: u32, _: f32, _: f32) -> [u8; 4] {
    [byte((frame + 1) as f32 / 4.0), 255, 255, 0]
}

fn data(_: u32, u: f32, _: f32) -> [u8; 4] {
    [byte(u), byte(u), byte(u), byte(u)]
}

const ELEMENTS: [FixtureElement; 4] = [
    FixtureElement {
        name: "soft_disc",
        frames: 1,
        blend: ElementBlend::Premultiplied,
        texel: soft_disc,
    },
    FixtureElement {
        name: "solid",
        frames: 1,
        blend: ElementBlend::Premultiplied,
        texel: solid,
    },
    FixtureElement {
        name: "flip",
        frames: 4,
        blend: ElementBlend::Additive,
        texel: flip,
    },
    FixtureElement {
        name: "data_sheet",
        frames: 1,
        blend: ElementBlend::Data,
        texel: data,
    },
];

/// Writes the fixture set into `dir` and returns its manifest path.
pub fn write_fixture_set(dir: &Path) -> PathBuf {
    write_named_set(dir, SET, CELL, MIPS, "")
}

/// Writes the fixture elements as set `set` with cells of `cell` pixels, `mips` levels and
/// every element name prefixed by `prefix`; returns the manifest path.
#[allow(clippy::cast_precision_loss)]
pub fn write_named_set(dir: &Path, set: &str, cell: u32, mips: u32, prefix: &str) -> PathBuf {
    let layers: u32 = ELEMENTS.iter().map(|element| element.frames).sum();
    let mut manifest = EffectTextureSet {
        format_version: EFFECT_FORMAT_VERSION,
        set: set.to_owned(),
        cell_size: cell,
        padding_px: 0,
        channels: ChannelLayout::PackedV1,
        layers,
        mip_files: Vec::new(),
        elements: Vec::new(),
    };
    let mut first_layer = 0;
    for element in &ELEMENTS {
        manifest.elements.push(EffectElement {
            name: format!("{prefix}{}", element.name),
            first_layer,
            frames: element.frames,
            duration_ms: (element.frames > 1).then_some(400),
            pivot_px: [cell / 2, cell / 2],
            blend: element.blend,
            channel_labels: None,
        });
        first_layer += element.frames;
    }
    for level in 0..mips {
        let side = cell >> level;
        let mut strip = image::RgbaImage::new(side, side * layers);
        let mut layer = 0;
        for element in &ELEMENTS {
            for frame in 0..element.frames {
                for y in 0..side {
                    for x in 0..side {
                        let u = (x as f32 + 0.5) / side as f32;
                        let v = (y as f32 + 0.5) / side as f32;
                        let texel = (element.texel)(frame, u, v);
                        strip.put_pixel(x, layer * side + y, image::Rgba(texel));
                    }
                }
                layer += 1;
            }
        }
        let name = mip_file_name(set, level as usize);
        strip
            .save(dir.join(&name))
            .expect("the fixture strip is written");
        manifest.mip_files.push(name);
    }
    let path = dir.join(format!("{set}.texture.ron"));
    let text = to_ron_text(&manifest, "fixture manifest").expect("the manifest encodes");
    std::fs::write(&path, text).expect("the fixture manifest is written");
    path
}

/// Name of the fixture noise set.
pub const NOISE_SET: &str = "fx_fixture_noise";

/// Writes a `NoiseV1` set of one element, `grain` (R rising with u, G with v, B and A 0), with
/// the fixture's cell size and mip count; returns the manifest path.
#[allow(clippy::cast_precision_loss)]
pub fn write_noise_set(dir: &Path) -> PathBuf {
    let mut manifest = EffectTextureSet {
        format_version: EFFECT_FORMAT_VERSION,
        set: NOISE_SET.to_owned(),
        cell_size: CELL,
        padding_px: 0,
        channels: ChannelLayout::NoiseV1,
        layers: 1,
        mip_files: Vec::new(),
        elements: vec![EffectElement {
            name: "grain".to_owned(),
            first_layer: 0,
            frames: 1,
            duration_ms: None,
            pivot_px: [CELL / 2, CELL / 2],
            blend: ElementBlend::Data,
            channel_labels: Some(["u".into(), "v".into(), "zero".into(), "zero".into()]),
        }],
    };
    for level in 0..MIPS {
        let side = CELL >> level;
        let mut strip = image::RgbaImage::new(side, side);
        for y in 0..side {
            for x in 0..side {
                let u = (x as f32 + 0.5) / side as f32;
                let v = (y as f32 + 0.5) / side as f32;
                strip.put_pixel(x, y, image::Rgba([unit_byte(u), unit_byte(v), 0, 0]));
            }
        }
        let name = mip_file_name(NOISE_SET, level as usize);
        strip
            .save(dir.join(&name))
            .expect("the noise strip is written");
        manifest.mip_files.push(name);
    }
    let path = dir.join(format!("{NOISE_SET}.texture.ron"));
    let text = to_ron_text(&manifest, "fixture noise manifest").expect("the manifest encodes");
    std::fs::write(&path, text).expect("the noise manifest is written");
    path
}

/// `value` in 0..1 as a byte.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// A vocabulary as [`vocabulary`] that lists both the fixture set and the noise set.
pub fn vocabulary_with_noise(
    manifest: &Path,
    noise: &Path,
    emitters: &str,
    effects: &str,
) -> String {
    format!(
        "(format_version: 1, textures: [{manifest:?}, {noise:?}], {RAMPS_AND_CURVES} \
         emitters: {{ {emitters} }}, effects: {{ {effects} }}, {BUDGET}, defaults: ())",
        manifest = manifest.display().to_string(),
        noise = noise.display().to_string()
    )
}

/// The standard budget section of fixture vocabularies.
pub const BUDGET: &str = "budget: (particles: 512, sprites: 256, decals: 64, lights: 4, \
     effects: 64, classes: { Major: 16, Minor: 32, Ambient: 16 }, min_screen_px: 0.0, \
     full_particle_px: 64.0, crowd_threshold: 24)";

/// The common ramps and curves of fixture vocabularies.
pub const RAMPS_AND_CURVES: &str = r#"
    ramps: {
        "white": [(0.0, (1.0, 1.0, 1.0, 1.0)), (1.0, (1.0, 1.0, 1.0, 1.0))],
        "red_add": [(0.0, (1.0, 0.0, 0.0, 0.0)), (1.0, (1.0, 0.0, 0.0, 0.0))],
        "green": [(0.0, (0.0, 1.0, 0.0, 1.0)), (1.0, (0.0, 1.0, 0.0, 1.0))],
        "blue": [(0.0, (0.0, 0.0, 1.0, 1.0)), (1.0, (0.0, 0.0, 1.0, 1.0))],
    },
    curves: {
        "flat": [(0.0, 1.0, Linear), (1.0, 1.0, Linear)],
        "fade": [(0.0, 1.0, Linear), (1.0, 0.0, Linear)],
    },
"#;

/// A complete vocabulary text: the fixture set at `manifest`, the common ramps and curves,
/// `emitters` and `effects` (RON map bodies), the standard budget and `defaults`.
pub fn vocabulary(manifest: &Path, emitters: &str, effects: &str, defaults: &str) -> String {
    format!(
        "(format_version: 1, textures: [{manifest:?}], {RAMPS_AND_CURVES} \
         emitters: {{ {emitters} }}, effects: {{ {effects} }}, {BUDGET}, defaults: ({defaults}))",
        manifest = manifest.display().to_string()
    )
}
