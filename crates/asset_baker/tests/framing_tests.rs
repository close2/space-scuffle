//! Framing of wide Veo clips into square game frames (plan
//! `docs/plans/2026-11-04-veo-letterbox-bake.md`, decisions D-1 to D-5).
//!
//! # What these tests pin
//! - Structures (`BuildingDome`) are letterboxed: the whole 16:9 frame is kept, with bars in a
//!   colour measured from the clip's own border, so nothing at the side edges is lost.
//! - Units (`UnitSphere`) keep the centre crop, byte for byte as before.
//! - The policy, its overrides and the bar colour are data and provenance, never Rust identity.
//!
//! # Fixtures
//! Each test writes a synthetic clip with the real `/usr/bin/ffmpeg` into a temporary
//! directory: a slate backdrop `(44, 48, 74)` with a saturated green column at each side edge
//! that spans only the middle rows, so the top and bottom rows are pure backdrop. A centre crop
//! removes both columns; a letterbox keeps them. Identities are chosen from the shipped catalog
//! by render shape, never by name.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

mod common;

use asset_baker::framing::frame_to_square;
use asset_baker::prompts::MEDIA_STYLE_FILE;
use asset_baker::provenance::{provenance_path, MediaProvenance};
use asset_baker::{
    bake_framed_action_videos, BakeFraming, BakerError, FrameSpec, FramedBake, MediaStyle,
};
use common::cli::{baker, baker_with_assets, shipped_assets, shipped_catalog, text};
use image::RgbaImage;
use sim_types::enums::{AnimState, ShapeKind};
use std::path::{Path, PathBuf};

/// The commit that added the original Veo clips; any well-formed commit would do here.
const LEGACY_COMMIT: &str = "3d9a9c4428803292e470bb339454c892566dd44f";

/// Backdrop of the synthetic clips, as sRGB bytes.
const BACKDROP: [u8; 3] = [44, 48, 74];

/// Side of the baked square frames in these tests, in pixels.
const SIZE: u32 = 64;

/// The first catalog identity with a `BuildingDome` shape.
fn structure_slug() -> String {
    shipped_catalog()
        .archetypes()
        .find(|archetype| matches!(archetype.shape, ShapeKind::BuildingDome { .. }))
        .map(|archetype| archetype.slug.clone())
        .expect("the shipped catalog has a structure")
}

/// Writes a `width`×`height`, 1 s, 8 fps `yuv420p` clip: the backdrop with a 12 px green
/// column at each side edge over the middle half of the rows.
fn edge_column_clip(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let output = dir.join(name);
    let [r, g, b] = BACKDROP;
    let top = height / 4 / 2 * 2;
    let rows = height / 2 / 2 * 2;
    let right = width - 12;
    let source = format!("color=c=0x{r:02X}{g:02X}{b:02X}:s={width}x{height}:r=8:d=1");
    let filter = format!(
        "drawbox=x=0:y={top}:w=12:h={rows}:color=0x00FF00:t=fill,\
         drawbox=x={right}:y={top}:w=12:h={rows}:color=0x00FF00:t=fill,format=yuv420p"
    );
    let status = std::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-v", "error", "-y", "-f", "lavfi", "-i", &source, "-vf", &filter,
        ])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&output)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg could not write {name}");
    output
}

/// Runs `bake-unit-videos` for `slug` with `clip` as its Idle clip and returns the atlas.
fn bake_idle(slug: &str, clip: &Path, videos: &Path, textures: &Path) -> RgbaImage {
    let output = baker()
        .args(["bake-unit-videos", "--unit", slug, "--idle"])
        .arg(clip)
        .arg("--videos-dir")
        .arg(videos)
        .arg("--output-dir")
        .arg(textures)
        .arg("--legacy-veo")
        .arg(format!("idle={LEGACY_COMMIT}"))
        .args(["--size", &SIZE.to_string(), "--fps", "8"])
        .output()
        .expect("run baker");
    assert!(output.status.success(), "{}", text(&output));
    image::open(textures.join(format!("{slug}.png")))
        .expect("atlas decodes")
        .to_rgba8()
}

/// Whether `pixel` is the saturated green of the fixture's edge columns.
fn is_green(pixel: image::Rgba<u8>) -> bool {
    pixel[1] > 150 && pixel[0] < 110 && pixel[2] < 110
}

/// Whether frame `index` of `atlas` has a green pixel within `columns` of the given side.
fn green_near_edge(atlas: &RgbaImage, index: u32, columns: std::ops::Range<u32>) -> bool {
    (index * SIZE..(index + 1) * SIZE)
        .any(|y| columns.clone().any(|x| is_green(*atlas.get_pixel(x, y))))
}

#[test]
fn a_structure_bake_keeps_the_edges_a_centre_crop_loses() {
    let slug = structure_slug();
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let clip = edge_column_clip(&videos, &format!("{slug}_idle.mp4"), 192, 108);
    let atlas = bake_idle(&slug, &clip, &videos, &root.path().join("textures"));
    let frames = atlas.height() / SIZE;
    assert!(frames > 0);
    for index in 0..frames {
        assert!(
            green_near_edge(&atlas, index, 0..6),
            "frame {index} lost the left edge column"
        );
        assert!(
            green_near_edge(&atlas, index, SIZE - 6..SIZE),
            "frame {index} lost the right edge column"
        );
    }
}

// --- Helpers for the tests below --------------------------------------------------------------

/// The first catalog identity with a `UnitSphere` shape.
fn unit_slug() -> String {
    shipped_catalog()
        .archetypes()
        .find(|archetype| archetype.shape == ShapeKind::UnitSphere)
        .map(|archetype| archetype.slug.clone())
        .expect("the shipped catalog has a unit")
}

/// Writes a `width`×`height`, 1 s, 8 fps `yuv420p` clip of the plain backdrop with a 16 px
/// green box in the middle, which touches no edge.
fn centred_box_clip(dir: &Path, name: &str, width: u32, height: u32) -> PathBuf {
    let output = dir.join(name);
    let [r, g, b] = BACKDROP;
    let source = format!("color=c=0x{r:02X}{g:02X}{b:02X}:s={width}x{height}:r=8:d=1");
    let (x, y) = (width / 2 - 8, height / 2 - 8);
    let filter = format!("drawbox=x={x}:y={y}:w=16:h=16:color=0x00FF00:t=fill,format=yuv420p");
    let status = std::process::Command::new("/usr/bin/ffmpeg")
        .args([
            "-v", "error", "-y", "-f", "lavfi", "-i", &source, "-vf", &filter,
        ])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p"])
        .arg(&output)
        .status()
        .expect("ffmpeg runs");
    assert!(status.success(), "ffmpeg could not write {name}");
    output
}

/// The provenance record that `bake-unit-videos` wrote for `slug` into `textures`.
fn record(textures: &Path, slug: &str) -> MediaProvenance {
    MediaProvenance::load(&provenance_path(textures, slug).unwrap()).expect("record loads")
}

/// A throw-away bundle: the shipped catalogs and media data, with `media_style.ron` edited.
fn bundle_with_style(edit: impl Fn(String) -> String) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for sub in ["catalog", "media"] {
        std::fs::create_dir_all(dir.path().join(sub)).unwrap();
        for entry in std::fs::read_dir(shipped_assets().join(sub)).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.path().join(sub).join(entry.file_name())).unwrap();
        }
    }
    let style = dir.path().join(MEDIA_STYLE_FILE);
    std::fs::write(&style, edit(std::fs::read_to_string(&style).unwrap())).unwrap();
    dir
}

/// The shipped style with `overrides: {}` replaced by `overrides`.
fn with_overrides(overrides: String) -> impl Fn(String) -> String {
    move |style: String| {
        assert!(
            style.contains("overrides: {},"),
            "the shipped style lists no override"
        );
        style.replace("overrides: {},", &format!("overrides: {overrides},"))
    }
}

/// Bakes `clip` as the Idle clip of `slug` through the library with `framing`, at [`SIZE`].
fn library_bake(slug: &str, clip: &Path, out: &Path, framing: BakeFraming) -> FramedBake {
    let spec = FrameSpec {
        size: SIZE,
        fps: 8,
        framing,
    };
    bake_framed_action_videos(slug, &[(AnimState::Idle, clip)], out, spec).expect("bake")
}

/// The shipped structure framing, written out so a change to the data cannot hide here.
const LETTERBOX_30: BakeFraming = BakeFraming::Letterbox {
    feather_permille: 30,
};

// --- Bars, the measured colour, and the shapes of clips ---------------------------------------

#[test]
fn the_bars_are_the_recorded_colour_and_that_colour_is_the_backdrop() {
    let slug = structure_slug();
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let clip = edge_column_clip(&videos, &format!("{slug}_idle.mp4"), 192, 108);
    let textures = root.path().join("textures");
    let atlas = bake_idle(&slug, &clip, &videos, &textures);

    let record = record(&textures, &slug);
    assert!(matches!(record.bake.framing, BakeFraming::Letterbox { .. }));
    let bar = record.states[&AnimState::Idle]
        .bar_rgb
        .expect("a letterbox records its bar colour");
    for (channel, (measured, wanted)) in bar.iter().zip(BACKDROP).enumerate() {
        assert!(
            measured.abs_diff(wanted) <= 4,
            "channel {channel}: {measured} is not within 4 of {wanted}"
        );
    }
    // 192×108 at 64 px is 64×36 content, centred: 14 bar rows above and below.
    let content_rows = 36;
    let bar_rows = (SIZE - content_rows) / 2;
    for index in 0..atlas.height() / SIZE {
        let top = index * SIZE;
        let bar_ys = (top..top + bar_rows).chain(top + bar_rows + content_rows..top + SIZE);
        for y in bar_ys {
            for x in 0..SIZE {
                let pixel = atlas.get_pixel(x, y);
                assert_eq!(pixel.0, [bar[0], bar[1], bar[2], 255], "({x}, {y})");
            }
        }
    }
}

#[test]
fn a_square_clip_bakes_byte_identically_under_both_framings() {
    let slug = structure_slug();
    let root = tempfile::tempdir().unwrap();
    let clip = centred_box_clip(root.path(), "square.mp4", 128, 128);
    let crop_dir = root.path().join("crop");
    let letterbox_dir = root.path().join("letterbox");
    let crop = library_bake(&slug, &clip, &crop_dir, BakeFraming::CenterCrop);
    let letterbox = library_bake(&slug, &clip, &letterbox_dir, LETTERBOX_30);
    assert_eq!(crop.bar_rgb[&AnimState::Idle], None);
    assert_eq!(letterbox.bar_rgb[&AnimState::Idle], None);
    let png = |dir: &Path| std::fs::read(dir.join(format!("{slug}.png"))).unwrap();
    assert_eq!(png(&crop_dir), png(&letterbox_dir));
}

#[test]
fn a_tall_clip_is_pillarboxed_with_bars_left_and_right() {
    let slug = structure_slug();
    let root = tempfile::tempdir().unwrap();
    let clip = centred_box_clip(root.path(), "tall.mp4", 108, 192);
    let out = root.path().join("tall");
    let baked = library_bake(&slug, &clip, &out, LETTERBOX_30);
    let bar = baked.bar_rgb[&AnimState::Idle].expect("a tall clip gets bars");
    let atlas = image::open(out.join(format!("{slug}.png")))
        .unwrap()
        .to_rgba8();
    // 108×192 at 64 px is 36×64 content, centred: 14 bar columns left and right.
    let (content, side) = (36, (SIZE - 36) / 2);
    for y in 0..atlas.height() {
        for x in (0..side).chain(side + content..SIZE) {
            let pixel = atlas.get_pixel(x, y);
            assert_eq!([pixel[0], pixel[1], pixel[2]], bar, "({x}, {y})");
        }
    }
    let middle = atlas.get_pixel(SIZE / 2, SIZE / 2);
    assert!(is_green(*middle), "the content stays centred: {middle:?}");
}

/// A 512×288 content frame: colour `edge` in its outermost 4 rows (the measured band at 512),
/// colour `inner` everywhere else.
fn banded_frame(edge: [u8; 3], inner: [u8; 3]) -> RgbaImage {
    RgbaImage::from_fn(512, 288, |_, y| {
        let [r, g, b] = if !(4..284).contains(&y) { edge } else { inner };
        image::Rgba([r, g, b, 255])
    })
}

#[test]
fn the_feather_blends_only_its_rows_monotonically_toward_the_bar_colour() {
    let (edge, inner) = ([40, 44, 70], [200, 120, 20]);
    let framed = frame_to_square(vec![banded_frame(edge, inner)], 512, LETTERBOX_30).unwrap();
    assert_eq!(framed.bar_rgb, Some(edge));
    let square = &framed.frames[0];
    let top = (512 - 288) / 2;
    let feather = LETTERBOX_30.feather_pixels(512);
    assert_eq!(feather, 15);
    let distance = |pixel: &image::Rgba<u8>| -> u32 {
        (0..3).map(|c| u32::from(pixel[c].abs_diff(edge[c]))).sum()
    };
    let mut previous = 0;
    for d in 0..144 {
        let pixel = square.get_pixel(256, top + d);
        let mirrored = square.get_pixel(256, top + 287 - d);
        assert_eq!(pixel, mirrored, "the feather is symmetric at row {d}");
        if d >= feather {
            assert_eq!(
                [pixel[0], pixel[1], pixel[2]],
                inner,
                "row {d} is untouched"
            );
        } else if d >= 4 {
            assert_ne!(
                [pixel[0], pixel[1], pixel[2]],
                inner,
                "row {d} is feathered"
            );
        }
        assert!(
            distance(pixel) >= previous,
            "row {d} moves away from the bar"
        );
        previous = distance(pixel);
    }

    let hard = BakeFraming::Letterbox {
        feather_permille: 0,
    };
    let framed = frame_to_square(vec![banded_frame(edge, inner)], 512, hard).unwrap();
    for d in 4..144 {
        let pixel = framed.frames[0].get_pixel(256, top + d);
        assert_eq!(
            [pixel[0], pixel[1], pixel[2]],
            inner,
            "row {d}: a hard edge"
        );
    }
}

// --- The policy is data ------------------------------------------------------------------------

#[test]
fn a_unit_keeps_the_centre_crop_and_records_it() {
    let slug = unit_slug();
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let clip = edge_column_clip(&videos, &format!("{slug}_idle.mp4"), 192, 108);
    let textures = root.path().join("textures");
    let atlas = bake_idle(&slug, &clip, &videos, &textures);
    let record = record(&textures, &slug);
    assert_eq!(record.bake.framing, BakeFraming::CenterCrop);
    assert_eq!(record.states[&AnimState::Idle].bar_rgb, None);
    assert!(
        !green_near_edge(&atlas, 0, 0..6),
        "the crop cuts the edge column"
    );
}

#[test]
fn an_override_in_data_letterboxes_one_unit() {
    let slug = unit_slug();
    let bundle = bundle_with_style(with_overrides(format!(
        "{{\"{slug}\": Letterbox(feather_permille: 0)}}"
    )));
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    let clip = edge_column_clip(&videos, &format!("{slug}_idle.mp4"), 192, 108);
    let textures = root.path().join("textures");
    let output = baker_with_assets(bundle.path())
        .args(["bake-unit-videos", "--unit", &slug, "--idle"])
        .arg(&clip)
        .arg("--videos-dir")
        .arg(&videos)
        .arg("--output-dir")
        .arg(&textures)
        .arg("--legacy-veo")
        .arg(format!("idle={LEGACY_COMMIT}"))
        .args(["--size", &SIZE.to_string(), "--fps", "8"])
        .output()
        .expect("run baker");
    assert!(output.status.success(), "{}", text(&output));
    assert_eq!(
        record(&textures, &slug).bake.framing,
        BakeFraming::Letterbox {
            feather_permille: 0
        }
    );
}

#[test]
fn an_override_naming_no_catalog_identity_is_a_typed_load_error() {
    let unknown = format!("{}_not_in_the_catalog", unit_slug());
    let bundle = bundle_with_style(with_overrides(format!("{{\"{unknown}\": CenterCrop}}")));
    let error = MediaStyle::load_for_catalog(bundle.path(), &shipped_catalog())
        .expect_err("an unknown override is refused");
    assert!(
        matches!(&error, BakerError::UnknownFramingOverride { slug, .. } if *slug == unknown),
        "{error}"
    );
    let output = baker_with_assets(bundle.path())
        .args([
            "bake-unit-videos",
            "--unit",
            &unit_slug(),
            "--idle",
            "none.mp4",
        ])
        .output()
        .expect("run baker");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains(&unknown),
        "{}",
        text(&output)
    );
}

#[test]
fn the_shipped_policy_letterboxes_structures_and_crops_units() {
    let catalog = shipped_catalog();
    let style = MediaStyle::load_for_catalog(&shipped_assets(), &catalog).expect("style loads");
    assert!(style.framing.overrides.is_empty());
    assert_eq!(style.framing.unit_sphere, BakeFraming::CenterCrop);
    assert!(matches!(
        style.framing.building_dome,
        BakeFraming::Letterbox { .. }
    ));
    let structure = catalog.get_by_slug(&structure_slug()).unwrap();
    assert_eq!(
        style.framing.framing_for(&structure.slug, &structure.shape),
        style.framing.building_dome
    );
}

// --- The review shows the shipped square -------------------------------------------------------

#[test]
fn review_writes_a_framed_sheet_of_letterboxed_squares_for_a_structure() {
    let slug = structure_slug();
    let root = tempfile::tempdir().unwrap();
    let videos = root.path().join("videos");
    std::fs::create_dir_all(&videos).unwrap();
    edge_column_clip(&videos, &format!("{slug}_idle.mp4"), 192, 108);
    let ledger = root.path().join("ledger.jsonl");
    std::fs::write(&ledger, "").unwrap();
    let (sheets, evidence) = (root.path().join("sheets"), root.path().join("evidence"));
    let output = baker()
        .args(["review", "--unit", &slug, "--videos-dir"])
        .arg(&videos)
        .arg("--ledger")
        .arg(&ledger)
        .arg("--sheets-dir")
        .arg(&sheets)
        .arg("--evidence-dir")
        .arg(&evidence)
        .output()
        .expect("run baker");
    assert!(output.status.success(), "{}", text(&output));
    let name = format!("{slug}_idle_framed.png");
    let sheet = image::open(sheets.join(&name)).unwrap().to_rgba8();
    let size = asset_baker::framing::DEFAULT_UNIT_FRAME_SIZE;
    assert_eq!(sheet.dimensions(), (8 * size, size));
    let bar = *sheet.get_pixel(size / 2, 0);
    for tile in 0..8 {
        let x0 = tile * size;
        let column = x0 + size / 2;
        assert_eq!(*sheet.get_pixel(column, 0), bar, "tile {tile}: top bar");
        assert_eq!(
            *sheet.get_pixel(column, size - 1),
            bar,
            "tile {tile}: bottom bar"
        );
        assert!(
            (x0..x0 + 8).any(|x| is_green(*sheet.get_pixel(x, size / 2))),
            "tile {tile} keeps the left edge column"
        );
    }
    let copy = image::open(evidence.join(&name)).unwrap();
    assert!(copy.width() <= 1024);
    assert!(
        sheets.join(format!("{slug}_idle.png")).is_file(),
        "the 16:9 sheet stays"
    );
}
