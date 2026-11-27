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
//! - M-4: the marks follow the world's presence rule under fog, are sized from footprints and
//!   radii, coloured by player or team and drawn resources, structures, units.
//! - M-11: the sandbox has no minimap at all; the default layout puts it inside the command panel
//!   with the world rectangle unchanged; a layout without the panel draws it alone, reserving
//!   nothing.
//! - The shipped shader parses and validates as WGSL, with the entry points the pipelines name.
//!
//! Marks are compared bit for bit: the builder is a pure function, so a mark's centre and colour
//! are exactly what the layout and the palette give.
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
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::single_match_else
)]

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use std::path::{Path, PathBuf};

use battle_harness::{Harness, LOCAL_PLAYER};
use bevy::prelude::*;
use bevy::render::render_resource::TextureUsages;
use client::battle_hud::{HudGeometry, HudLayout};
use client::battle_pipeline::fog::BattleFog;
use client::battle_pipeline::session::BattleSession;
use client::battle_selection::BattleSelection;
use client::minimap::layout::{MinimapLayout, EDGE_INSET};
use client::minimap::marks::{build_marks, MarkSources};
use client::minimap::style::{MinimapStyle, MinimapStyleError, MINIMAP_STYLE_PATH};
use client::minimap::terrain::{write_cell_texels, CELL_TEXEL_BYTES};
use client::minimap::{ColorMode, MinimapFrame, MinimapMark, MinimapSettings, MINIMAP_FORMAT};
use client::player_presentation::PlayerPalette;
use client::runtime_battle::MapPresentation;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::terrain::TerrainState;
use client::{ClientMode, RtsCamera};
use sim_map::grid::{CellFlags, GameMap, TerrainKind};
use sim_map::validation::CellRect;
use sim_types::snapshot::BubbleRenderInstance;
use sim_types::{Concealment, EntityId, PlayerId};

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

/// Whether a cell with `flags` and `kind` is walkable ground: the G channel's definition.
fn walkable(flags: CellFlags, kind: TerrainKind) -> bool {
    flags.contains(CellFlags::GROUND_PATHABLE) && kind != TerrainKind::Abyss
}

/// M-2′ (Amendment 1, A1.9): the terrain encodes no structure. Under every start structure of
/// every player on the shipped skirmish, G is the cell's walkability before placement, which is
/// walkable ground, so the enemy's start structure is not drawn into the terrain.
#[test]
fn m2_prime_the_terrain_shows_no_start_structure() {
    let mut harness = skirmish();
    harness.step(4);
    let terrain = harness.app.world().resource::<TerrainState>();
    let map = &terrain.map;
    let texels = frame(&harness).cells();
    let owners: std::collections::BTreeSet<PlayerId> = harness
        .app
        .world()
        .resource::<BattleSession>()
        .world()
        .expect("a live world")
        .entities
        .values()
        .filter(|entity| map.structures().contains_key(&entity.id))
        .map(|entity| entity.owner)
        .collect();
    assert!(
        owners.contains(&LOCAL_PLAYER) && owners.contains(&ENEMY),
        "both players start with structures: {owners:?}"
    );
    let (mut checked, mut open) = (0, 0);
    for placed in map.structures().values() {
        let mut prior = placed.prior_flags.iter();
        for dy in 0..u32::from(placed.height) {
            for dx in 0..u32::from(placed.width) {
                let (x, y) = (placed.top_left.0 + dx, placed.top_left.1 + dy);
                let flags = *prior.next().expect("one prior flag per footprint cell");
                let kind = map.cell(x, y).expect("in bounds").terrain_kind;
                let green = texel(texels, map, x, y)[1];
                assert_eq!(
                    green == u8::MAX,
                    walkable(flags, kind),
                    "structure {:?} cell ({x}, {y}): G follows the prior flags",
                    placed.id
                );
                checked += 1;
                open += usize::from(green == u8::MAX);
            }
        }
    }
    assert!(
        checked > 0 && open == checked,
        "every start structure stands on walkable ground, drawn as such: {open} of {checked}"
    );
}

/// M-2′: an authored unwalkable cell beside a structure stays unwalkable, and the structure's own
/// cells read their prior flags.
#[test]
fn m2_prime_an_unwalkable_cell_beside_a_structure_stays_unwalkable() {
    use sim_map::footprint::StructureFootprint;
    use sim_map::grid::{CliffLevel, MapCell};
    let mut map = GameMap::new(8, 8, MapCell::default_ground(CliffLevel(0))).expect("a map");
    map.cell_mut(5, 3)
        .expect("in bounds")
        .flags
        .remove(CellFlags::GROUND_PATHABLE);
    map.place_structure(EntityId(7), (2, 2), StructureFootprint::square(3, 3))
        .expect("the footprint is open");
    let mut texels = Vec::new();
    write_cell_texels(&map, &mut texels);
    for y in 2..5 {
        for x in 2..5 {
            assert_eq!(
                texel(&texels, &map, x, y)[1],
                u8::MAX,
                "({x}, {y}) under it"
            );
        }
    }
    assert_eq!(
        texel(&texels, &map, 5, 3)[1],
        0,
        "the authored cell beside it"
    );
    assert_eq!(texel(&texels, &map, 6, 3)[1], u8::MAX, "open ground");
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

// --- M-4: marks under fog -----------------------------------------------------------------------

const ENEMY: PlayerId = PlayerId(1);

/// `owner`'s mobile units, ordered by ID.
fn mobile(harness: &Harness, owner: PlayerId) -> Vec<EntityId> {
    harness
        .world()
        .entities
        .values()
        .filter(|unit| unit.owner == owner && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect()
}

/// Runs `ticks` simulation ticks, then pauses.
fn run_ticks(harness: &mut Harness, ticks: u32) {
    let goal = harness.world().current_tick + ticks;
    harness.resume();
    let mut frames = 0;
    while harness.world().current_tick < goal {
        harness.step(1);
        frames += 1;
        assert!(frames < 10_000, "the battle stopped ticking");
    }
    harness.pause();
}

/// The presented bubble of `id`.
fn presented(harness: &Harness, id: EntityId) -> Option<BubbleRenderInstance> {
    harness
        .app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .bubbles
        .iter()
        .find(|bubble| bubble.entity_id == id)
        .cloned()
}

/// The snapped pixel centre a mark of map point `world` has.
fn centre_of(layout: &MinimapLayout, world: Vec2) -> [f32; 2] {
    let pixel = layout.to_minimap(world);
    [pixel.x.floor() + 0.5, pixel.y.floor() + 0.5]
}

/// The index of the mark of bubble `id`: the mark at its centre in `color`.
fn mark_of(harness: &Harness, id: EntityId, color: [f32; 4]) -> Option<usize> {
    let bubble = presented(harness, id)?;
    let frame = frame(harness);
    let layout = frame.layout()?;
    let centre = centre_of(&layout, Vec2::new(bubble.world_pos[0], bubble.world_pos[1]));
    frame
        .marks()
        .iter()
        .position(|mark| mark.a == centre && mark.color == color)
}

fn palette_color(harness: &Harness, owner: PlayerId) -> [f32; 4] {
    harness
        .app
        .world()
        .resource::<PlayerPalette>()
        .resolve(owner)
        .expect("a palette owner")
        .linear
        .to_array()
}

/// The marks the builder makes from the harness's current state, built twice.
fn build_twice(
    harness: &Harness,
    edit: impl Fn(&mut Vec<BubbleRenderInstance>),
) -> [Vec<MinimapMark>; 2] {
    let world = harness.app.world();
    let session = world.resource::<BattleSession>();
    let mut bubbles = session.frames.presented().bubbles.clone();
    edit(&mut bubbles);
    let fog = world.resource::<BattleFog>();
    let frame = world.resource::<MinimapFrame>();
    let settings = world.resource::<MinimapSettings>();
    let sources = MarkSources {
        layout: frame.layout().expect("a layout"),
        style: world.resource::<MinimapStyle>(),
        mode: settings.color_mode,
        viewer: fog.viewer(),
        fog: Some(fog),
        bubbles: &bubbles,
        catalog: Some(&harness.world().catalog),
        palette: Some(world.resource::<PlayerPalette>()),
        objects: &world.resource::<MapPresentation>().objects,
        nodes: world.resource::<TerrainState>().map.resources(),
        selected: world.resource::<BattleSelection>().ids(),
    };
    let (mut first, mut second) = (Vec::new(), Vec::new());
    build_marks(&sources, &mut first);
    build_marks(&sources, &mut second);
    [first, second]
}

#[test]
fn m4_marks_follow_the_worlds_presence_rule() {
    let mut harness = skirmish();
    run_ticks(&mut harness, 2);
    let enemy = palette_color(&harness, ENEMY);
    let own = palette_color(&harness, LOCAL_PLAYER);
    // The enemy's base is in the fog: none of it is listed (the red test: without the presence
    // filter the builder lists it).
    assert!(
        frame(&harness)
            .marks()
            .iter()
            .all(|mark| mark.color != enemy),
        "no enemy mark while every enemy is in the fog"
    );
    // One's own units and structures always are.
    let locals: Vec<EntityId> = harness
        .world()
        .entities
        .values()
        .filter(|unit| unit.owner == LOCAL_PLAYER)
        .map(|unit| unit.id)
        .collect();
    for id in &locals {
        assert!(
            mark_of(&harness, *id, own).is_some(),
            "own {id:?} is listed"
        );
    }

    // Stage, beside home: an undetected burrowed enemy, an undetected cloaked enemy, and a
    // cloaked enemy beside a local detector; and reveal one enemy worker by its own attack.
    let workers = mobile(&harness, LOCAL_PLAYER);
    let home = harness.simulated_position(workers[0]);
    let burrowed_at = harness.open_ground(home, 5.0);
    let burrowed = harness.spawn_concealed(ENEMY, burrowed_at, Concealment::Burrowed);
    let cloaked_at = harness.open_ground(home, 8.0);
    let cloaked = harness.spawn_concealed(ENEMY, cloaked_at, Concealment::Cloaked);
    let revealed = mobile(&harness, ENEMY)[0];
    let slot = LOCAL_PLAYER.slot().expect("the viewer is seated");
    harness
        .world_mut()
        .entities
        .get_mut(&revealed)
        .expect("the enemy worker lives")
        .attack_reveal_ticks[slot] = 1_000;
    run_ticks(&mut harness, 3);

    assert_eq!(
        mark_of(&harness, burrowed, enemy),
        None,
        "undetected burrowed"
    );
    assert_eq!(
        mark_of(&harness, cloaked, enemy),
        None,
        "undetected cloaked"
    );
    // A cloaked enemy beside a local detector is detected, and listed.
    let detected_at = harness.open_ground(home, 11.0);
    let detected = harness.spawn_concealed(ENEMY, detected_at, Concealment::Cloaked);
    let detector_type = harness.detector();
    let beside = harness.open_ground(detected_at, 2.0);
    harness
        .world_mut()
        .spawn_unit(
            detector_type,
            LOCAL_PLAYER,
            battle_harness::sim_position(beside),
        )
        .expect("open ground");
    run_ticks(&mut harness, 3);
    assert!(
        mark_of(&harness, detected, enemy).is_some(),
        "detected cloaked"
    );
    assert!(
        mark_of(&harness, revealed, enemy).is_some(),
        "attack-revealed"
    );
    let still_fogged = mobile(&harness, ENEMY)
        .into_iter()
        .filter(|id| *id != revealed)
        .find(|id| {
            let at = harness.simulated_position(*id);
            !harness.app.world().resource::<BattleFog>().sees(at.x, at.y)
        })
        .expect("another enemy worker stays in the fog");
    assert_eq!(mark_of(&harness, still_fogged, enemy), None, "fogged enemy");

    // A housed harvester and a hangar member are never listed: the same bubbles, one housed and
    // one given an archetype that cannot be picked.
    let unpickable = harness
        .world()
        .catalog
        .archetypes()
        .find(|arch| !(arch.selectable && arch.order_targetable))
        .expect("the catalog ships an unpickable archetype (a hangar member)")
        .unit_type_id;
    let [housed, _] = build_twice(&harness, |bubbles| {
        for bubble in bubbles.iter_mut().filter(|b| b.entity_id == workers[0]) {
            bubble.housed = true;
        }
        for bubble in bubbles.iter_mut().filter(|b| b.entity_id == workers[1]) {
            bubble.unit_type = unpickable;
        }
    });
    let layout = frame(&harness).layout().expect("a layout");
    for worker in &workers[..2] {
        let at = presented(&harness, *worker).expect("presented").world_pos;
        let centre = centre_of(&layout, Vec2::new(at[0], at[1]));
        let others_there = workers[2..].iter().any(|other| {
            presented(&harness, *other).is_some_and(|b| {
                centre_of(&layout, Vec2::new(b.world_pos[0], b.world_pos[1])) == centre
            })
        });
        if !others_there {
            assert!(
                housed
                    .iter()
                    .all(|mark| !(mark.a == centre && mark.color == own)),
                "housed or unpickable {worker:?} is not listed"
            );
        }
    }

    // Two builds from the same inputs are equal, and equal the frame's own list.
    let [first, second] = build_twice(&harness, |_| {});
    assert_eq!(first, second);
    assert_eq!(first.as_slice(), frame(&harness).marks());
}

#[test]
fn m4_marks_are_sized_ordered_and_coloured() {
    let mut harness = skirmish();
    run_ticks(&mut harness, 2);
    let layout = frame(&harness).layout().expect("a layout");
    let style = harness.app.world().resource::<MinimapStyle>().clone();
    let own = palette_color(&harness, LOCAL_PLAYER);
    let marks = frame(&harness).marks().to_vec();
    // Sizes: a structure's half-width is half its footprint's larger side, a unit's its radius,
    // at least half the minimum dot.
    let mut structure_index = None;
    let mut unit_index = None;
    for bubble in harness
        .app
        .world()
        .resource::<BattleSession>()
        .frames
        .presented()
        .bubbles
        .iter()
        .filter(|bubble| bubble.owner == LOCAL_PLAYER)
    {
        let index = mark_of(&harness, bubble.entity_id, own).expect("own bubble listed");
        let half = marks[index].b[0];
        match bubble.shape.footprint() {
            Some((w, h)) => {
                let expected = f32::from(w.max(h)) * 0.5 * layout.scale;
                assert!(
                    (half - expected).abs() < 1e-4,
                    "structure {half} vs {expected}"
                );
                structure_index = structure_index.max(Some(index));
                if (w, h) == (5, 5) {
                    assert!((half - 2.5 * layout.scale).abs() < 1e-4);
                }
            }
            None => {
                let expected = (bubble.radius * layout.scale).max(style.min_dot_px * 0.5);
                assert!((half - expected).abs() < 1e-4, "unit {half} vs {expected}");
                unit_index = Some(unit_index.map_or(index, |seen: usize| seen.min(index)));
            }
        }
        assert_eq!(marks[index].stroke, style.border_px);
    }
    // Order: resources, then structures, then units.
    let last_resource = marks
        .iter()
        .rposition(|mark| mark.color == style.resource.linear)
        .expect("resources are listed");
    let (structure_index, unit_index) = (
        structure_index.expect("a local structure"),
        unit_index.expect("a local unit"),
    );
    assert!(
        last_resource < structure_index,
        "resources before structures"
    );
    assert!(structure_index < unit_index, "structures before units");

    // Team mode: own units `self`, the selected one `selected`.
    let workers = mobile(&harness, LOCAL_PLAYER);
    harness.look_at_unit(workers[0]);
    harness.select(workers[0]);
    harness
        .app
        .world_mut()
        .resource_mut::<MinimapSettings>()
        .color_mode = ColorMode::Team;
    harness.step(2);
    assert!(
        mark_of(&harness, workers[0], style.team.selected.linear).is_some(),
        "selected"
    );
    assert!(
        mark_of(&harness, workers[1], style.team.own.linear).is_some(),
        "own"
    );
    assert!(
        frame(&harness).marks().iter().all(|mark| mark.color != own),
        "no palette colour in team mode"
    );
    // Back to player colours through Alt+F.
    harness.with_key_held(KeyCode::AltLeft, |harness| harness.tap_key(KeyCode::KeyF));
    harness.step(2);
    assert_eq!(
        harness.app.world().resource::<MinimapSettings>().color_mode,
        ColorMode::Player
    );
    assert!(
        mark_of(&harness, workers[0], own).is_some(),
        "player colour again"
    );
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
