//! Behavioural tests of the effect-texture pipeline (plan
//! `docs/plans/2026-11-09-effect-visualization.md` §6, WP5) through its public API and CLI.
//!
//! # Responsibility
//! Proves that keying recovers known alpha (the difference matte within 1/255), that the black
//! check rejects a lit floor, a horizon and rays, that authored noise is deterministic and
//! tileable, that a bake writes a set whose every element cites its ledger line and which
//! verifies, that a source without a ledger line or a changed file is caught, that a clip's
//! effect span is chosen through the real FFmpeg importer, and that the review writes its
//! contact sheets.
//!
//! All output goes to temporary directories. Synthetic pictures here are test fixtures and
//! never reach `assets/`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::unwrap_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::doc_markdown
)]

use asset_baker::effects::bake::{
    bake_noise, bake_recipe, BakeInputs, EffectBakeRecipe, ElementLook, ElementRecipe,
    ElementSource,
};
use asset_baker::effects::keying::{check_black, difference_matte, MATTE_RESIDUAL_LIMIT};
use asset_baker::effects::noise::{
    generate_sheet, ChannelSpec, CurlComponent, Fractal, NoiseChannels, NoiseElementSpec,
    NoiseRecipe,
};
use asset_baker::effects::review::review_set;
use asset_baker::effects::texture_set::{
    list_sets, load_ron, owned_files, verify_set, ChannelLayout, EffectSetProvenance,
    EffectTextureSet, ElementBlend, ElementOrigin, KeyingMethod, SetProblem,
};
use asset_baker::effects::EffectError;
use asset_baker::ledger::{
    read_ledger_numbered, sha256_file, ApiMode, ImageRequestKind, Ledger, LedgerEvent,
    NumberedRecord,
};
use asset_baker::VeoParameters;
use image::{Rgba, RgbaImage};
use sim_types::enums::AnimState;
use std::path::{Path, PathBuf};

mod common;

const SET: &str = "fx_test";
const MODEL: &str = "fixture-image-model";

/// A smooth radial alpha in `[0, 1]`: 1 in the middle, 0 beyond `radius`.
fn known_alpha(x: u32, y: u32, size: u32, radius: f64) -> f64 {
    let c = f64::from(size) / 2.0;
    let d = ((f64::from(x) + 0.5 - c).powi(2) + (f64::from(y) + 0.5 - c).powi(2)).sqrt();
    (1.0 - d / radius).clamp(0.0, 1.0)
}

/// The same element composited (on stored 8-bit values) over black and over white.
fn pair(size: u32) -> (RgbaImage, RgbaImage) {
    let mut black = RgbaImage::new(size, size);
    let mut white = RgbaImage::new(size, size);
    for y in 0..size {
        for x in 0..size {
            let a = known_alpha(x, y, size, f64::from(size) * 0.4);
            let colour = [0.9, 0.6 + 0.3 * f64::from(x) / f64::from(size), 0.4];
            let over = |bg: f64| {
                Rgba(std::array::from_fn(|k| {
                    if k == 3 {
                        255
                    } else {
                        ((a * colour[k] + (1.0 - a) * bg) * 255.0).round() as u8
                    }
                }))
            };
            black.put_pixel(x, y, over(0.0));
            white.put_pixel(x, y, over(1.0));
        }
    }
    (black, white)
}

/// A black picture with a soft grey disc in the middle.
fn soft_disc(size: u32, level: f64) -> RgbaImage {
    RgbaImage::from_fn(size, size, |x, y| {
        let v = (known_alpha(x, y, size, f64::from(size) * 0.3) * level * 255.0).round() as u8;
        Rgba([v, v, v, 255])
    })
}

#[test]
fn the_difference_matte_reproduces_known_alpha_within_one_step() {
    let size = 96;
    let (black, white) = pair(size);
    let (keyed, report) = difference_matte(&black, &white, "puff").expect("pair agrees");
    assert_eq!(report.over_limit, 0);
    assert!(report.max_residual <= MATTE_RESIDUAL_LIMIT);
    for y in 0..size {
        for x in 0..size {
            let expected = known_alpha(x, y, size, f64::from(size) * 0.4);
            let got = f64::from(keyed.density.data[(y * size + x) as usize]);
            assert!(
                (got - expected).abs() <= 1.0 / 255.0 + 1e-6,
                "alpha at ({x}, {y}): {got} vs {expected}"
            );
        }
    }
}

#[test]
fn a_pair_that_disagrees_is_rejected_by_its_residual() {
    let (black, mut white) = pair(64);
    // A white render whose green channel is darker over half the frame: no single alpha
    // explains the three channels.
    for (x, _, pixel) in white.enumerate_pixels_mut() {
        if x < 32 {
            pixel[1] = pixel[1].saturating_sub(40);
        }
    }
    assert!(matches!(
        difference_matte(&black, &white, "puff"),
        Err(EffectError::MatteRejected { .. })
    ));
    let small = RgbaImage::new(32, 32);
    assert!(matches!(
        difference_matte(&black, &small, "puff"),
        Err(EffectError::PairSizeMismatch { .. })
    ));
}

#[test]
fn the_black_check_rejects_a_lit_floor_a_horizon_and_rays() {
    let clean = soft_disc(128, 0.9);
    check_black(&clean, "e", None).expect("a centred element on black passes");

    let mut floor = clean.clone();
    for y in 96..128 {
        for x in 0..128 {
            let v = 30 + (y - 96) as u8 * 2;
            floor.put_pixel(x, y, Rgba([v, v, v, 255]));
        }
    }
    let mut horizon = clean.clone();
    for y in 62..66 {
        for x in 0..128 {
            horizon.put_pixel(x, y, Rgba([60, 60, 60, 255]));
        }
    }
    let mut rays = clean.clone();
    for t in 0..128_i64 {
        for w in -1..=1_i64 {
            for (x, y) in [(t, t + w), (t, 127 - t + w), (64 + w, t), (t, 64 + w)] {
                if (0..128).contains(&x) && (0..128).contains(&y) {
                    rays.put_pixel(x as u32, y as u32, Rgba([80, 80, 80, 255]));
                }
            }
        }
    }
    for (name, picture) in [("floor", floor), ("horizon", horizon), ("rays", rays)] {
        assert!(
            matches!(
                check_black(&picture, name, None),
                Err(EffectError::BlackCheckFailed { .. })
            ),
            "{name} must fail the black check"
        );
    }
}

fn noise_channels() -> NoiseChannels {
    let fractal = Fractal {
        seed: 11,
        period: 4,
        octaves: 4,
        gain_permille: 500,
    };
    NoiseChannels {
        r: ChannelSpec::Perlin(fractal),
        g: ChannelSpec::Worley {
            seed: 3,
            cells: 6,
            invert: false,
        },
        b: ChannelSpec::Curl {
            potential: fractal,
            component: CurlComponent::X,
        },
        a: ChannelSpec::Value(fractal),
    }
}

#[test]
fn authored_noise_is_deterministic_and_tiles() {
    let channels = noise_channels();
    let first = generate_sheet(&channels, 64);
    assert_eq!(
        first,
        generate_sheet(&channels, 64),
        "same parameters, same pixels"
    );
    let other = NoiseChannels {
        r: ChannelSpec::Perlin(Fractal {
            seed: 12,
            period: 4,
            octaves: 4,
            gain_permille: 500,
        }),
        ..channels
    };
    assert_ne!(first, generate_sheet(&other, 64), "the seed matters");
    // Across the wrap, neighbouring pixels differ no more than neighbours inside the sheet.
    for k in [0_usize, 1, 3] {
        let step = |a: u32, b: u32, y: u32| {
            i32::from(first.get_pixel(a, y)[k]) - i32::from(first.get_pixel(b, y)[k])
        };
        let inner = (0..64).map(|y| step(31, 32, y).abs()).max().unwrap_or(0);
        let wrap = (0..64).map(|y| step(63, 0, y).abs()).max().unwrap_or(0);
        assert!(
            wrap <= inner.max(4) * 2,
            "channel {k}: wrap {wrap}, inner {inner}"
        );
    }
}

#[test]
fn a_noise_set_is_written_and_regenerates_from_its_parameters() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe = NoiseRecipe {
        set: "fx_noise_test".to_string(),
        cell_size: 64,
        elements: vec![
            NoiseElementSpec {
                name: "noise_tile".to_string(),
                channels: noise_channels(),
            },
            NoiseElementSpec {
                name: "ring_noise".to_string(),
                channels: NoiseChannels {
                    r: ChannelSpec::RingMask {
                        inner_permille: 500,
                        outer_permille: 800,
                        softness_permille: 60,
                    },
                    g: ChannelSpec::ErosionGradient {
                        noise: Fractal {
                            seed: 5,
                            period: 3,
                            octaves: 3,
                            gain_permille: 500,
                        },
                        noise_permille: 300,
                    },
                    b: ChannelSpec::Constant(0),
                    a: ChannelSpec::Constant(255),
                },
            },
        ],
    };
    let (manifest, provenance) = asset_baker::effects::texture_set::write_set(
        dir.path(),
        bake_noise(&recipe).expect("bake"),
    )
    .expect("write");
    assert_eq!(manifest.channels, ChannelLayout::NoiseV1);
    assert_eq!(manifest.layers, 2);
    assert_eq!(manifest.mip_files.len(), 3, "64, 32 and 16 pixels");
    assert_eq!(
        manifest.elements[0].channel_labels.clone().expect("labels"),
        ["perlin", "worley", "curl_x", "value"].map(String::from)
    );
    assert!(matches!(
        provenance.elements["noise_tile"].origin,
        ElementOrigin::Authored { .. }
    ));
    assert_eq!(verify_set(dir.path(), "fx_noise_test", &[]), Vec::new());

    // Change the recorded parameters: the sheet no longer reproduces.
    let (_, provenance_path) =
        asset_baker::effects::texture_set::set_paths(dir.path(), "fx_noise_test").expect("paths");
    let mut edited: EffectSetProvenance = load_ron(&provenance_path).expect("provenance");
    if let Some(record) = edited.elements.get_mut("ring_noise") {
        if let ElementOrigin::Authored { params, .. } = &mut record.origin {
            params.channels.b = ChannelSpec::Constant(1);
        }
    }
    std::fs::write(
        &provenance_path,
        asset_baker::effects::texture_set::to_ron_text(&edited, "p").expect("ron"),
    )
    .expect("write");
    assert_eq!(
        verify_set(dir.path(), "fx_noise_test", &[]),
        vec![SetProblem::AuthoredMismatch {
            element: "ring_noise".to_string()
        }]
    );
}

/// Writes the ledger lines an image-model request would have written for `file`.
fn record_image(ledger: &mut Ledger, element: &str, file: &Path, kind: ImageRequestKind) {
    let request_id = ledger.next_request_id();
    ledger
        .append(LedgerEvent::ImageRequestStarted {
            request_id: request_id.clone(),
            set: SET.to_string(),
            element: element.to_string(),
            model: MODEL.to_string(),
            mode: ApiMode::VertexExpress,
            kind,
            prompt_sha256: "a".repeat(64),
            prompt: "prompt".to_string(),
            source_sha256: None,
        })
        .expect("started");
    let (width, height) = image::image_dimensions(file).expect("dims");
    ledger
        .append(LedgerEvent::ImageDownloaded {
            request_id,
            set: SET.to_string(),
            element: element.to_string(),
            model: MODEL.to_string(),
            bytes: std::fs::metadata(file).expect("meta").len(),
            sha256: sha256_file(file).expect("sha"),
            width,
            height,
            mime_type: "image/png".to_string(),
            path: file.display().to_string(),
        })
        .expect("downloaded");
}

/// Sources of three still elements and their ledger.
struct StillFixture {
    dir: tempfile::TempDir,
    ledger: Vec<NumberedRecord>,
    recipe: EffectBakeRecipe,
}

impl StillFixture {
    fn sources(&self) -> PathBuf {
        self.dir.path().join("sources")
    }
    fn out(&self) -> PathBuf {
        self.dir.path().join("effects")
    }
}

fn still_fixture() -> StillFixture {
    let dir = tempfile::tempdir().expect("tempdir");
    let sources = dir.path().join("sources");
    std::fs::create_dir_all(&sources).expect("sources");
    let (black, white) = pair(200);
    black.save(sources.join("puff_black.png")).expect("png");
    white.save(sources.join("puff_white.png")).expect("png");
    soft_disc(180, 1.0)
        .save(sources.join("glow.png"))
        .expect("png");
    soft_disc(220, 0.5)
        .save(sources.join("stain.png"))
        .expect("png");
    let ledger_path = dir.path().join("ledger.jsonl");
    let mut ledger = Ledger::open(&ledger_path, "run".to_string()).expect("ledger");
    record_image(
        &mut ledger,
        "glow",
        &sources.join("glow.png"),
        ImageRequestKind::OnBlack,
    );
    record_image(
        &mut ledger,
        "puff",
        &sources.join("puff_black.png"),
        ImageRequestKind::OnBlack,
    );
    record_image(
        &mut ledger,
        "puff",
        &sources.join("puff_white.png"),
        ImageRequestKind::OnWhiteEdit,
    );
    record_image(
        &mut ledger,
        "stain",
        &sources.join("stain.png"),
        ImageRequestKind::OnBlack,
    );
    let element = |name: &str, look, source| ElementRecipe {
        name: name.to_string(),
        look,
        source,
    };
    let recipe = EffectBakeRecipe {
        set: SET.to_string(),
        cell_size: 256,
        erosion: None,
        elements: vec![
            element(
                "glow",
                ElementLook::Emissive,
                ElementSource::Still {
                    file: "glow.png".into(),
                },
            ),
            element(
                "puff",
                ElementLook::Lit,
                ElementSource::Pair {
                    black: "puff_black.png".into(),
                    white: "puff_white.png".into(),
                },
            ),
            element(
                "stain",
                ElementLook::Lit,
                ElementSource::Still {
                    file: "stain.png".into(),
                },
            ),
        ],
    };
    StillFixture {
        ledger: read_ledger_numbered(&ledger_path).expect("read ledger"),
        dir,
        recipe,
    }
}

fn bake_stills(fixture: &StillFixture, ledger: &[NumberedRecord]) -> Result<(), EffectError> {
    let sources = fixture.sources();
    let baked = bake_recipe(
        &fixture.recipe,
        BakeInputs {
            sources_dir: &sources,
            ledger,
            ffmpeg: Path::new("/usr/bin/ffmpeg"),
        },
    )?;
    asset_baker::effects::texture_set::write_set(&fixture.out(), baked)?;
    Ok(())
}

#[test]
fn a_baked_set_packs_every_element_and_verifies_against_its_ledger() {
    let fixture = still_fixture();
    bake_stills(&fixture, &fixture.ledger).expect("bake");
    let out = fixture.out();
    assert_eq!(verify_set(&out, SET, &fixture.ledger), Vec::new());
    assert_eq!(list_sets(&out).expect("list"), vec![SET.to_string()]);

    let manifest: EffectTextureSet = load_ron(&out.join("fx_test.texture.ron")).expect("manifest");
    assert_eq!(manifest.layers, 3);
    assert_eq!(manifest.padding_px, 2);
    assert_eq!(manifest.mip_files.len(), 5, "256 down to 16 pixels");
    assert_eq!(manifest.elements[0].blend, ElementBlend::Additive);
    assert_eq!(manifest.elements[1].blend, ElementBlend::Premultiplied);
    assert_eq!(manifest.elements[1].pivot_px, [128, 128]);
    assert_eq!(
        owned_files(&out, SET).expect("files").len(),
        2 + manifest.mip_files.len()
    );

    let strip = image::open(out.join("fx_test.png"))
        .expect("strip")
        .to_rgba8();
    assert_eq!(strip.dimensions(), (256, 768));
    let mip = image::open(out.join("fx_test.mip4.png"))
        .expect("mip")
        .to_rgba8();
    assert_eq!(mip.dimensions(), (16, 48));
    // Glow: emissive mask set at its centre; puff: lit, dense, not emissive; padding empty.
    let glow_centre = strip.get_pixel(128, 128);
    assert_eq!(glow_centre[3], 255);
    assert!(glow_centre[1] > 200, "{glow_centre:?}");
    let puff_centre = strip.get_pixel(128, 256 + 128);
    assert_eq!(puff_centre[3], 0);
    assert!(puff_centre[1] > 240, "{puff_centre:?}");
    for layer in 0..3 {
        for i in 0..256 {
            for j in 0..2 {
                assert_eq!(strip.get_pixel(i, layer * 256 + j)[1], 0);
                assert_eq!(strip.get_pixel(j, layer * 256 + i)[1], 0);
            }
        }
    }
    // Erosion order: the centre erodes last, the rim first.
    assert!(puff_centre[2] > strip.get_pixel(20, 256 + 128)[2]);

    let provenance: EffectSetProvenance =
        load_ron(&out.join("fx_test.provenance.ron")).expect("provenance");
    let puff = &provenance.elements["puff"];
    assert_eq!(puff.keying.method, KeyingMethod::DifferenceMatte);
    assert_eq!(puff.keying.matte.expect("matte").over_limit, 0);
    match &puff.origin {
        ElementOrigin::GeneratedImage { sources } => {
            assert_eq!(sources.len(), 2);
            assert_eq!(sources[0].model, MODEL);
            assert_eq!(sources[1].ledger_line, 6);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        provenance.elements["stain"].keying.method,
        KeyingMethod::LuminanceKey
    );
    assert_eq!(
        provenance.elements["glow"].keying.method,
        KeyingMethod::LuminanceEmissive
    );
}

#[test]
fn a_source_without_a_ledger_line_is_refused_and_a_changed_file_is_caught() {
    let fixture = still_fixture();
    let without_puff: Vec<NumberedRecord> = fixture
        .ledger
        .iter()
        .filter(|n| !matches!(&n.record.event, LedgerEvent::ImageDownloaded { element, .. } if element == "puff"))
        .cloned()
        .collect();
    assert!(matches!(
        bake_stills(&fixture, &without_puff),
        Err(EffectError::OriginMissing {
            wanted: "image_downloaded",
            ..
        })
    ));
    assert!(
        !fixture.out().exists(),
        "nothing is written for a refused bake"
    );

    bake_stills(&fixture, &fixture.ledger).expect("bake");
    std::fs::write(fixture.out().join("fx_test.mip2.png"), b"changed").expect("tamper");
    assert!(
        verify_set(&fixture.out(), SET, &fixture.ledger).contains(&SetProblem::HashMismatch {
            file: "fx_test.mip2.png".to_string()
        })
    );
    // A ledger that no longer holds the cited lines cannot verify the origins.
    assert!(verify_set(&fixture.out(), SET, &[])
        .iter()
        .any(|p| matches!(p, SetProblem::OriginUnverified { .. })));
}

#[test]
fn recipes_refuse_emissive_pairs_odd_cell_sizes_and_duplicates() {
    let fixture = still_fixture();
    let mut recipe = fixture.recipe.clone();
    recipe.cell_size = 300;
    assert!(matches!(
        recipe.check(Path::new("r")),
        Err(EffectError::InvalidRecipe { .. })
    ));
    let mut recipe = fixture.recipe.clone();
    recipe.elements[1].look = ElementLook::Emissive;
    assert!(matches!(
        recipe.check(Path::new("r")),
        Err(EffectError::InvalidRecipe { .. })
    ));
    let mut recipe = fixture.recipe.clone();
    recipe.elements[2].name = "glow".to_string();
    assert!(matches!(
        recipe.check(Path::new("r")),
        Err(EffectError::InvalidRecipe { .. })
    ));
    let mut recipe = fixture.recipe;
    recipe.set = "../escape".to_string();
    assert!(matches!(
        recipe.check(Path::new("r")),
        Err(EffectError::InvalidName { .. })
    ));
}

/// A clip that starts black, shows a growing then fading glow, and ends black.
fn effect_frames() -> Vec<RgbaImage> {
    let mut frames = Vec::new();
    for index in 0..30_u32 {
        let level = match index {
            0..=4 | 25..=29 => 0.0,
            5..=14 => f64::from(index - 4) / 10.0,
            _ => f64::from(25 - index) / 10.0,
        };
        frames.push(soft_disc(96, level));
    }
    frames
}

#[test]
fn a_clip_flipbook_keeps_frames_over_its_effect_span() {
    let dir = tempfile::tempdir().expect("tempdir");
    let sources = dir.path().join("sources");
    std::fs::create_dir_all(&sources).expect("sources");
    let clip = common::loop_clip::encode_clip(&sources, "glow.mkv", &effect_frames());
    let ledger_path = dir.path().join("ledger.jsonl");
    let mut ledger = Ledger::open(&ledger_path, "run".to_string()).expect("ledger");
    let started = |request_id: &str| LedgerEvent::RequestStarted {
        request_id: request_id.to_string(),
        slug: "fx_test_glow".to_string(),
        state: AnimState::Idle,
        model: "fixture-veo".to_string(),
        mode: ApiMode::VertexExpress,
        prompt_sha256: "b".repeat(64),
        prompt: "p".to_string(),
        keyframe_sha256: None,
        last_frame_sha256: None,
        parameters: VeoParameters::default(),
    };
    ledger.append(started("run/1")).expect("line");
    ledger
        .append(LedgerEvent::OperationNamed {
            request_id: "run/1".to_string(),
            slug: "fx_test_glow".to_string(),
            state: AnimState::Idle,
            model: "fixture-veo".to_string(),
            operation: "operations/glow".to_string(),
        })
        .expect("line");
    ledger
        .append(LedgerEvent::Downloaded {
            slug: "fx_test_glow".to_string(),
            state: AnimState::Idle,
            operation: "operations/glow".to_string(),
            bytes: 1,
            sha256: sha256_file(&clip).expect("sha"),
            duration_seconds: 2.0,
            frames: 30,
            width: 96,
            height: 96,
            path: clip.display().to_string(),
        })
        .expect("line");
    let numbered = read_ledger_numbered(&ledger_path).expect("ledger");
    let recipe = EffectBakeRecipe {
        set: SET.to_string(),
        cell_size: 256,
        erosion: None,
        elements: vec![ElementRecipe {
            name: "glow".to_string(),
            look: ElementLook::Emissive,
            source: ElementSource::Clip {
                file: "glow.mkv".to_string(),
                frames: 6,
                decode_fps: common::loop_clip::CLIP_FPS,
            },
        }],
    };
    let baked = bake_recipe(
        &recipe,
        BakeInputs {
            sources_dir: &sources,
            ledger: &numbered,
            ffmpeg: Path::new("/usr/bin/ffmpeg"),
        },
    )
    .expect("bake");
    let out = dir.path().join("effects");
    let (manifest, provenance) =
        asset_baker::effects::texture_set::write_set(&out, baked).expect("write");
    let span = provenance.elements["glow"].span.clone().expect("span");
    assert_eq!(span.decoded_frames, 30);
    assert!((5..=6).contains(&span.onset), "{span:?}");
    assert!((23..=24).contains(&span.dissipation), "{span:?}");
    assert_eq!(span.kept.len(), 6);
    assert_eq!(manifest.elements[0].frames, 6);
    assert_eq!(manifest.elements[0].duration_ms, Some(span.duration_ms));
    assert!(matches!(
        &provenance.elements["glow"].origin,
        ElementOrigin::Veo { ledger_line: 3, operation, .. } if operation == "operations/glow"
    ));
    assert!(provenance.ffmpeg_version.is_some());
    assert_eq!(verify_set(&out, SET, &numbered), Vec::new());

    let review = dir.path().join("review");
    let swatch = dir.path().join("swatch.png");
    RgbaImage::from_pixel(32, 32, Rgba([90, 120, 60, 255]))
        .save(&swatch)
        .expect("swatch");
    let output = review_set(&out, SET, &[swatch.clone(), swatch], &review).expect("review");
    let sheet = image::open(&output.sheet).expect("sheet").to_rgba8();
    assert_eq!(
        sheet.width(),
        4 + 10 * (128 + 4),
        "ten tiles per packed element"
    );
    let frames = image::open(output.frames.expect("frame strip")).expect("frames");
    assert_eq!(frames.width(), 4 + 6 * (64 + 4));
    assert!(std::fs::read_to_string(&output.legend)
        .expect("legend")
        .contains("glow (6 frame(s)"));
}

#[test]
fn the_cli_prints_a_checked_effect_prompt_and_writes_a_noise_set() {
    let output = common::cli::baker()
        .args([
            "effect-prompt",
            "--set",
            "fx_test",
            "--element",
            "smoke_puff",
            "--description",
            "A dense billowing smoke puff.",
            "--view",
            "top-down",
            "--medium",
            "clip",
        ])
        .output()
        .expect("run");
    let text = common::cli::text(&output);
    assert!(output.status.success(), "{text}");
    assert!(
        text.contains("A dense billowing smoke puff. Seen from directly above"),
        "{text}"
    );
    assert!(text.contains("pure black background"), "{text}");

    let refused = common::cli::baker()
        .args([
            "effect-prompt",
            "--set",
            "fx_test",
            "--element",
            "e",
            "--description",
            "A blast from Warcraft.",
        ])
        .output()
        .expect("run");
    assert!(!refused.status.success());

    let dir = tempfile::tempdir().expect("tempdir");
    let recipe = dir.path().join("noise.ron");
    std::fs::write(
        &recipe,
        "(set: \"fx_cli_noise\", cell_size: 32, elements: [(name: \"tile\", channels: (r: Perlin((seed: 1, period: 2, octaves: 2, gain_permille: 500)), g: Constant(0), b: Constant(0), a: Constant(255)))])",
    )
    .expect("recipe");
    let out = dir.path().join("effects");
    let written = common::cli::baker()
        .args(["effect-noise", "--recipe"])
        .arg(&recipe)
        .arg("--output-dir")
        .arg(&out)
        .output()
        .expect("run");
    assert!(written.status.success(), "{}", common::cli::text(&written));
    assert_eq!(verify_set(&out, "fx_cli_noise", &[]), Vec::new());
}
