//! The minimap through public client APIs and the production plugin, without a GPU (plan
//! `2026-12-01-minimap` §6, M-1 to M-4 and M-11).
//!
//! # Responsibility
//! - M-1: [`MinimapLayout`] maps the map onto the image and back, letterboxes a map that is not
//!   square and clamps to the camera's bound.
//! - M-2: the cell texels of the shipped maps encode height, walkability and cliff edges exactly,
//!   and the production frame rebuilds them only when the terrain's revision moves.
//! - M-3: the shipped `minimap.ron` loads from the runtime path, and a bad value names the file
//!   and the field.
//! - M-11: the sandbox has no minimap at all; the default layout puts it inside the command panel
//!   with the world rectangle unchanged; a layout without the panel draws it alone, reserving
//!   nothing.
//! - The shipped shader parses and validates as WGSL, with the entry points the pipelines name.
//!
//! # How it fits
//! The app tests boot the production client with `support/battle_harness.rs` (no window, no
//! renderer; egui runs and records rectangles). Pixels are proven in `minimap_gpu_tests.rs`.

#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use std::path::{Path, PathBuf};

use battle_harness::Harness;
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use client::battle_hud::{HudGeometry, HudLayout};
use client::minimap::layout::{MinimapLayout, EDGE_INSET};
use client::minimap::style::{MinimapStyle, MinimapStyleError, MINIMAP_STYLE_PATH};
use client::minimap::terrain::{write_cell_texels, CELL_TEXEL_BYTES};
use client::minimap::{MinimapFrame, MINIMAP_FORMAT};
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::terrain::TerrainState;
use client::{ClientMode, RtsCamera};
use sim_map::grid::{CellFlags, GameMap, TerrainKind};
use sim_map::validation::CellRect;

fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

// --- M-1: the layout -----------------------------------------------------------------------------

#[test]
fn m1_the_layout_round_trips_letterboxes_and_clamps() {
    for (width, height, side) in [
        (64_u32, 64_u32, 172.0_f32),
        (128, 128, 172.0),
        (96, 64, 172.0),
    ] {
        let layout = MinimapLayout::new(width, height, side).expect("a layout");
        let (w, h) = (width as f32, height as f32);
        for i in 0..=16 {
            for j in 0..=16 {
                let point = Vec2::new(w * i as f32 / 16.0, h * j as f32 / 16.0);
                let back = layout.to_world(layout.to_minimap(point));
                assert!(
                    back.distance(point) < 1e-4,
                    "{width}x{height}: {point} came back as {back}"
                );
            }
        }
        // The map's corners map to the edges of the map rectangle, which touches the image's
        // edges along the longer side and the letterbox along the shorter.
        let origin = layout.to_minimap(Vec2::ZERO);
        let far = layout.to_minimap(Vec2::new(w, h));
        assert_eq!(origin, layout.offset);
        assert!((far.x - (side - layout.offset.x)).abs() < 1e-3, "{far}");
        assert!((far.y - (side - layout.offset.y)).abs() < 1e-3, "{far}");
        assert!(layout.contains(origin + Vec2::splat(0.5)));
        // The camera's own bound.
        let clamped = layout.to_world_clamped(Vec2::new(-50.0, side + 50.0));
        assert_eq!(clamped, Vec2::new(EDGE_INSET, h - EDGE_INSET));
    }
    let wide = MinimapLayout::new(96, 64, 172.0).expect("a layout");
    assert!(
        wide.offset.y > 1.0,
        "a wide map is letterboxed: {:?}",
        wide.offset
    );
    assert_eq!(wide.offset.x, 0.0);
    assert!(
        !wide.contains(Vec2::new(86.0, wide.offset.y * 0.5)),
        "the top band"
    );
    assert!(
        !wide.contains(Vec2::new(86.0, 172.0 - wide.offset.y * 0.5)),
        "the bottom band"
    );
    assert!(wide.contains(Vec2::new(86.0, 86.0)));
}

// --- M-2: the cell texels ------------------------------------------------------------------------

/// The shipped map at `path` beneath the assets root.
fn shipped_map(path: &str) -> GameMap {
    sim_map::map_format::MapFile::load(&assets().join(path))
        .expect("the shipped map loads")
        .decode()
        .expect("the shipped map decodes")
        .to_game_map()
        .expect("the shipped map builds")
}

/// The texel of cell `(x, y)`.
fn texel(texels: &[u8], map: &GameMap, x: u32, y: u32) -> [u8; 4] {
    let index = (y * map.width() + x) as usize * CELL_TEXEL_BYTES;
    texels[index..index + CELL_TEXEL_BYTES]
        .try_into()
        .expect("four bytes")
}

/// The cells of `map` as `(x, y, cell)`.
fn cells(map: &GameMap) -> impl Iterator<Item = (u32, u32, &sim_map::grid::MapCell)> + '_ {
    (0..map.height()).flat_map(move |y| {
        (0..map.width()).map(move |x| (x, y, map.cell(x, y).expect("in bounds")))
    })
}

/// Whether flat cell `(x, y)` has a flat 4-neighbour on a lower level: the definition, written
/// again here so that the test does not read the code it checks.
fn expected_edge(map: &GameMap, x: u32, y: u32) -> bool {
    let cell = map.cell(x, y).expect("in bounds");
    if cell.ramp.is_some() {
        return false;
    }
    let neighbours = [
        (x.wrapping_add(1), y),
        (x.wrapping_sub(1), y),
        (x, y.wrapping_add(1)),
        (x, y.wrapping_sub(1)),
    ];
    neighbours.into_iter().any(|(nx, ny)| {
        map.cell(nx, ny)
            .is_ok_and(|other| other.ramp.is_none() && other.cliff_level < cell.cliff_level)
    })
}

#[test]
fn m2_the_cell_texels_encode_the_shipped_maps() {
    for path in [
        "maps/showcase_canyon.scmap.ron",
        "maps/default_arena.scmap.ron",
    ] {
        let map = shipped_map(path);
        let mut texels = Vec::new();
        write_cell_texels(&map, &mut texels);
        assert_eq!(
            texels.len(),
            (map.width() * map.height()) as usize * CELL_TEXEL_BYTES
        );
        let mut level_red: [Option<u8>; 4] = [None; 4];
        let (mut ramps, mut edges) = (0, 0);
        for (x, y, cell) in cells(&map) {
            let [red, green, blue, alpha] = texel(&texels, &map, x, y);
            assert_eq!(alpha, 255);
            let walkable = cell.flags.contains(CellFlags::GROUND_PATHABLE)
                && cell.terrain_kind != TerrainKind::Abyss;
            assert_eq!(green == 255, walkable, "{path} ({x}, {y}) G");
            assert!(green == 0 || green == 255);
            let edge = expected_edge(&map, x, y);
            assert_eq!(blue == 255, edge, "{path} ({x}, {y}) B");
            edges += usize::from(edge);
            let level = usize::from(cell.cliff_level.0);
            if cell.ramp.is_none() {
                // Every flat cell of a level holds the same height.
                let seen = level_red[level].get_or_insert(red);
                assert_eq!(*seen, red, "{path} ({x}, {y}) R on level {level}");
            } else {
                ramps += 1;
                let low = (level as f32 / 3.0 * 255.0).round() as u8;
                let high = ((level + 1) as f32 / 3.0 * 255.0).round() as u8;
                assert!(
                    low < red && red < high,
                    "{path} ramp ({x}, {y}): R {red} not strictly between {low} and {high}"
                );
            }
        }
        let flat: Vec<u8> = level_red.iter().flatten().copied().collect();
        assert!(flat.len() >= 2, "{path} has several levels: {level_red:?}");
        assert!(
            flat.windows(2).all(|pair| pair[0] < pair[1]),
            "{path}: R orders with the level: {level_red:?}"
        );
        assert!(
            ramps > 0 && edges > 0,
            "{path}: {ramps} ramps, {edges} edges"
        );
    }
}

/// Boots the shipped skirmish under the player's profile.
fn skirmish() -> Harness {
    Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO)
}

fn frame(harness: &Harness) -> &MinimapFrame {
    harness.app.world().resource::<MinimapFrame>()
}

#[test]
fn m2_the_frame_rebuilds_cells_only_when_the_revision_moves() {
    let mut harness = skirmish();
    harness.step(4);
    let built = frame(&harness).cells_generation();
    assert!(built > 0, "the cells are built for the battle's map");
    let expected = {
        let terrain = harness.app.world().resource::<TerrainState>();
        let mut texels = Vec::new();
        write_cell_texels(&terrain.map, &mut texels);
        texels
    };
    assert_eq!(frame(&harness).cells(), expected.as_slice());
    harness.resume();
    harness.step(20);
    assert_eq!(
        frame(&harness).cells_generation(),
        built,
        "ticks, frames and camera moves do not rebuild the cells"
    );
    harness
        .app
        .world_mut()
        .resource_mut::<TerrainState>()
        .mark_dirty(CellRect {
            min: (0, 0),
            max_inclusive: (0, 0),
        });
    harness.step(2);
    assert_eq!(frame(&harness).cells_generation(), built + 1);
    harness.step(4);
    assert_eq!(frame(&harness).cells_generation(), built + 1);
}

// --- M-3: the style ------------------------------------------------------------------------------

#[test]
fn m3_the_shipped_style_loads_from_the_runtime_path() {
    let root = client::ResolvedAssetsRoot::from_directory(&assets()).expect("the assets root");
    let style = MinimapStyle::load(root.as_path()).expect("the shipped minimap.ron is valid");
    assert!(style.side_points > 0.0);
    assert_eq!(style.level_brightness.len(), 4);
}

#[test]
fn m3_each_bad_value_names_the_file_and_the_field() {
    let path = assets().join(MINIMAP_STYLE_PATH);
    let shipped = std::fs::read_to_string(&path).expect("the shipped file");
    let cases = [
        (
            "background: (0.0, 0.0, 0.0)",
            "background: (0.0, 1.5, 0.0)",
            "background",
        ),
        (
            "level_brightness: [0.55, 0.70, 0.85, 1.0]",
            "level_brightness: [0.55, 0.70, 0.85]",
            "level_brightness",
        ),
        (
            "unpathable_color: (0.06, 0.06, 0.08)",
            "unpathable_color: (0.06, NaN, 0.08)",
            "unpathable_color",
        ),
        ("edge_mix: 0.6", "edge_mix: 1.6", "edge_mix"),
    ];
    for (good, bad, field) in cases {
        assert!(shipped.contains(good), "the shipped file holds `{good}`");
        let text = shipped.replace(good, bad);
        match MinimapStyle::from_text(&path, &text) {
            Err(error @ MinimapStyleError::Value { field: named, .. }) => {
                assert_eq!(named, field);
                let message = error.to_string();
                assert!(message.contains("minimap.ron"), "{message}");
                assert!(message.contains(field), "{message}");
            }
            other => panic!("`{bad}` must be refused as a bad `{field}`, got {other:?}"),
        }
    }
    let missing = MinimapStyle::load(Path::new("/nonexistent-minimap-root"));
    assert!(
        matches!(
            &missing,
            Err(MinimapStyleError::Io { path, .. }) if path.ends_with(MINIMAP_STYLE_PATH)
        ),
        "{missing:?}"
    );
}

// --- M-11: where it sits -------------------------------------------------------------------------

fn hud(harness: &Harness) -> &HudGeometry {
    harness.app.world().resource::<HudGeometry>()
}

/// The world camera's viewport as `(x, y, width, height)` in physical pixels.
fn viewport(harness: &mut Harness) -> (u32, u32, u32, u32) {
    let world = harness.app.world_mut();
    let camera = world
        .query_filtered::<&Camera, With<RtsCamera>>()
        .single(world);
    let viewport = camera.viewport.clone().expect("a world viewport");
    (
        viewport.physical_position.x,
        viewport.physical_position.y,
        viewport.physical_size.x,
        viewport.physical_size.y,
    )
}

/// Images shaped like the minimap's render target.
fn minimap_like_images(harness: &Harness) -> usize {
    harness
        .app
        .world()
        .resource::<Assets<Image>>()
        .iter()
        .filter(|(_, image)| {
            image.texture_descriptor.format == MINIMAP_FORMAT
                && image
                    .texture_descriptor
                    .usage
                    .contains(TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC)
        })
        .count()
}

#[test]
fn m11_the_sandbox_has_no_minimap() {
    let harness = Harness::battle();
    let world = harness.app.world();
    assert!(world.get_resource::<MinimapFrame>().is_none(), "no frame");
    assert!(world.get_resource::<MinimapStyle>().is_none(), "no style");
    let schedules = world.resource::<Schedules>();
    let names: Vec<String> = schedules
        .iter()
        .flat_map(|(_, schedule)| {
            schedule
                .graph()
                .systems()
                .map(|(_, system, _)| system.name().into_owned())
                .collect::<Vec<_>>()
        })
        .filter(|name| name.contains("minimap"))
        .collect();
    assert_eq!(names, Vec::<String>::new(), "no minimap system");
    assert_eq!(minimap_like_images(&harness), 0, "no minimap image");
    assert_eq!(hud(&harness).minimap, None);
}

#[test]
fn m11_the_default_layout_puts_the_minimap_in_the_panel_and_keeps_the_world() {
    let mut harness = skirmish();
    harness.step(2);
    let geometry = hud(&harness);
    let bottom = geometry.bottom.expect("the command panel");
    let minimap = geometry.minimap.expect("the minimap is drawn");
    let selection = geometry.selection_column.expect("the selection column");
    assert!(
        bottom.contains_rect(minimap),
        "{minimap:?} inside {bottom:?}"
    );
    assert!(minimap.right() <= selection.left(), "left of the columns");
    assert!(
        (minimap.left() - bottom.left()) < 16.0,
        "at the bottom left"
    );
    let style = harness.app.world().resource::<MinimapStyle>();
    assert_eq!(minimap.width(), style.side_points);
    assert_eq!(minimap.height(), style.side_points);
    // The base commit's world rectangle at 1280x720: below the 28-point resource bar, above the
    // 180-point command panel.
    assert_eq!(viewport(&mut harness), (0, 28, 1280, 512));
    assert_eq!(minimap_like_images(&harness), 1, "one image");
    let created = frame(&harness).images_created();
    harness.step(10);
    assert_eq!(
        frame(&harness).images_created(),
        created,
        "none in the steady state"
    );
}

#[test]
fn m11_without_the_panel_the_minimap_stands_alone_and_reserves_nothing() {
    let mut harness = skirmish();
    harness.app.insert_resource(HudLayout {
        command_panel: false,
        minimap: true,
    });
    harness.step(2);
    let geometry = hud(&harness);
    assert_eq!(geometry.bottom, None, "no command panel");
    let minimap = geometry.minimap.expect("the minimap is drawn alone");
    assert_eq!(minimap.left(), 0.0);
    assert_eq!(minimap.bottom(), 720.0);
    let top = geometry.top.expect("the resource bar");
    assert_eq!(
        viewport(&mut harness),
        (
            0,
            top.bottom().ceil() as u32,
            1280,
            720 - top.bottom().ceil() as u32
        ),
        "the world reaches the bottom of the window"
    );
    harness.app.insert_resource(HudLayout {
        command_panel: false,
        minimap: false,
    });
    harness.step(2);
    assert_eq!(hud(&harness).minimap, None, "and none at all without it");
}

// --- The shader ----------------------------------------------------------------------------------

#[test]
fn the_shipped_shader_validates_with_its_entry_points() {
    let source = std::fs::read_to_string(assets().join(client::minimap::render::MINIMAP_SHADER))
        .expect("the shipped shader");
    let module = naga::front::wgsl::parse_str(&source).expect("the shader parses");
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("the shader validates");
    let entries: Vec<&str> = module
        .entry_points
        .iter()
        .map(|entry| entry.name.as_str())
        .collect();
    for name in ["terrain_vs", "terrain_fs", "mark_vs", "mark_fs"] {
        assert!(entries.contains(&name), "{name} in {entries:?}");
    }
}
