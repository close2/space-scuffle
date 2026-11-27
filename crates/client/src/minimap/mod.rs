//! The minimap: a `StarCraft` II style overview of the battle in the bottom left of the HUD
//! (plan `docs/plans/2026-12-01-minimap.md`, B-204).
//!
//! # Responsibility
//! Owns one retained **minimap frame** in the main world ([`MinimapFrame`]): the layout that maps
//! the map onto the image ([`layout`]), the per-cell terrain texels ([`terrain`]), the image the
//! renderer draws into and the egui texture that shows it. The render world draws the frame with
//! one render-graph node and the real shader `assets/shaders/minimap.wgsl` ([`render`]); egui
//! shows the image in the command panel ([`panel`]). The look is data,
//! `assets/catalog/minimap.ron` ([`style`]).
//!
//! # How it fits
//! ```text
//! Startup             load_minimap_style
//! CollectIntent       ensure_minimap_image, before battle_hud_system (which draws the panel);
//!                     collect_minimap_keys (Alt+T, the terrain toggle)
//! Present             prepare_minimap_frame, after camera_control_system
//! ExtractSchedule     render::extract_minimap (clone_from into retained vectors)
//! Render              render::prepare_minimap_gpu (uploads on change only); MinimapNode
//!                     before the camera driver, and so before egui's window pass
//! ```
//! Everything here exists only under
//! [`BattleProfile::Player`](crate::battle_presentation::BattleProfile::Player): the sandbox, the
//! editor and the headless check present no player and have no minimap. Nothing here writes the
//! simulation; the minimap reads the published map and the presented snapshot only.
//!
//! # Invariants
//! - The image is created once per size (a window or scale change), never in the steady state, and
//!   no Bevy asset is modified per frame.
//! - The cell texels are rebuilt only when the session or the terrain's revision moves.
//! - Every retained vector keeps its storage across rebuilds of the same size.

pub mod layout;
pub mod panel;
pub mod render;
pub mod style;
pub mod terrain;

use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy_egui::{egui, EguiContexts};

use crate::assets::ResolvedAssetsRoot;
use crate::battle_hud::HudLayout;
use crate::battle_pipeline::commands::SessionGeneration;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_pipeline::BattleSet;
use crate::battle_presentation::BattleProfile;
use crate::input_bindings::{any_pressed, ALT_MODIFIERS, MINIMAP_TERRAIN_KEY};
use crate::input_policy::InputOwnership;
use crate::terrain::{TerrainFieldImages, TerrainMaterial, TerrainMaterialHandle, TerrainState};
use layout::MinimapLayout;
use style::{MinimapStyle, MinimapStyleStatus};

/// Format of the minimap image: linear colours written, sRGB stored, so egui, which samples it
/// as linear, shows exactly the colour the shader computed.
pub const MINIMAP_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// The player's minimap toggles.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapSettings {
    /// Whether the terrain is drawn (`StarCraft` II's `TerrainButton`); off, the map rectangle is
    /// the background and only what is drawn over it remains.
    pub terrain: bool,
}

impl Default for MinimapSettings {
    fn default() -> Self {
        Self { terrain: true }
    }
}

/// One shape on the minimap, in minimap pixels (plan `2026-12-01-minimap` D-5). Packed for the
/// storage buffer the marks pass reads.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MinimapMark {
    /// Centre (square, ring) or start point (segment), pixels from the target's top-left.
    pub a: [f32; 2],
    /// Half-width and half-height (square), outer radius and thickness (ring), or end point
    /// (segment).
    pub b: [f32; 2],
    /// Linear RGBA, premultiplied in the shader.
    pub color: [f32; 4],
    /// The mark's kind, as a number.
    pub kind: u32,
    /// Presented tick the mark's animation starts at (rings), else 0.
    pub start_tick: u32,
    /// Border width in pixels (squares) or line width (segments).
    pub stroke: f32,
    /// Padding to the 16-byte stride of the shader's struct; always 0.
    pub pad: f32,
}

/// The retained minimap of the main world: what the renderer draws and egui shows.
///
// PERFORMANCE RATIONALE:
// Why: The minimap is drawn on every frame of a battle, and the charter forbids steady-state
// allocation in presentation.
// What: One resource whose vectors are refilled in place, each with a generation the render
// world compares before it uploads, so an unchanged frame copies and uploads nothing.
// Trade-off: The texels and marks are held twice (here and in the render world's mirror), at most
// 64 KiB of cells and tens of KiB of marks.
#[derive(Resource, Debug, Default)]
pub struct MinimapFrame {
    layout: Option<MinimapLayout>,
    side_points: f32,
    side_px: u32,
    image: Option<Handle<Image>>,
    texture: Option<egui::TextureId>,
    images_created: u64,
    cells: Vec<u8>,
    cells_generation: u64,
    cells_key: Option<CellsKey>,
    map_size: UVec2,
    albedo: Option<Handle<Image>>,
    splat: Option<Handle<Image>>,
    marks: Vec<MinimapMark>,
    marks_generation: u64,
    terrain_on: bool,
    tick: u32,
}

/// What the cell texels were built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CellsKey {
    generation: Option<SessionGeneration>,
    revision: u64,
    size: UVec2,
}

impl MinimapFrame {
    /// The mapping of the current map onto the image, once both are known.
    #[must_use]
    pub const fn layout(&self) -> Option<MinimapLayout> {
        self.layout
    }

    /// The image the renderer draws into, once created.
    #[must_use]
    pub const fn image(&self) -> Option<&Handle<Image>> {
        self.image.as_ref()
    }

    /// The egui texture showing [`Self::image`].
    #[must_use]
    pub const fn texture(&self) -> Option<egui::TextureId> {
        self.texture
    }

    /// Side of the minimap square in egui points, from the style.
    #[must_use]
    pub const fn side_points(&self) -> f32 {
        self.side_points
    }

    /// Side of the image in physical pixels.
    #[must_use]
    pub const fn side_px(&self) -> u32 {
        self.side_px
    }

    /// How many images were created, for tests that the steady state creates none.
    #[must_use]
    pub const fn images_created(&self) -> u64 {
        self.images_created
    }

    /// The cell texels, `map_size.x × map_size.y` RGBA8 texels row-major ([`terrain`]).
    #[must_use]
    pub fn cells(&self) -> &[u8] {
        &self.cells
    }

    /// Bumped by every rebuild of [`Self::cells`].
    #[must_use]
    pub const fn cells_generation(&self) -> u64 {
        self.cells_generation
    }

    /// The map's size in cells, zero before a battle.
    #[must_use]
    pub const fn map_size(&self) -> UVec2 {
        self.map_size
    }

    /// The marks, in draw order.
    #[must_use]
    pub fn marks(&self) -> &[MinimapMark] {
        &self.marks
    }

    /// Bumped by every rebuild of [`Self::marks`].
    #[must_use]
    pub const fn marks_generation(&self) -> u64 {
        self.marks_generation
    }

    /// Whether the terrain is drawn this frame.
    #[must_use]
    pub const fn terrain_on(&self) -> bool {
        self.terrain_on
    }

    /// Every growable buffer this frame retains, as `(name, address, capacity in elements)`.
    ///
    /// Evidence for the steady-state tests that rebuilds reuse storage. Not part of the frame's
    /// interface.
    #[doc(hidden)]
    pub fn retained_buffers(&self) -> impl Iterator<Item = (&'static str, usize, usize)> + '_ {
        [
            (
                "minimap.cells",
                self.cells.as_ptr().addr(),
                self.cells.capacity(),
            ),
            (
                "minimap.marks",
                self.marks.as_ptr().addr(),
                self.marks.capacity(),
            ),
        ]
        .into_iter()
    }
}

/// Registers the minimap. Added by [`ClientAppPlugin`](crate::ClientAppPlugin) after the HUD;
/// adds nothing at all unless the battle is presented to a player.
pub struct MinimapPlugin;

impl Plugin for MinimapPlugin {
    fn build(&self, app: &mut App) {
        if app.world().get_resource::<BattleProfile>() != Some(&BattleProfile::Player) {
            return;
        }
        app.init_resource::<MinimapFrame>()
            .init_resource::<MinimapSettings>()
            .init_resource::<MinimapStyleStatus>()
            .add_systems(Startup, load_minimap_style)
            .add_systems(
                Update,
                (
                    ensure_minimap_image
                        .before(crate::battle_hud::panels::battle_hud_system)
                        .in_set(BattleSet::CollectIntent),
                    collect_minimap_keys.in_set(BattleSet::CollectIntent),
                    prepare_minimap_frame
                        .after(crate::camera_control_system)
                        .in_set(BattleSet::Present),
                    render::report_minimap_render_failure,
                ),
            );
        render::build_render(app);
    }

    fn finish(&self, app: &mut App) {
        if app.world().get_resource::<BattleProfile>() != Some(&BattleProfile::Player) {
            return;
        }
        render::finish_render(app);
    }
}

/// Loads the style from the runtime assets root, or records why it failed and ends the app with
/// an error: the minimap is never drawn from substitute values.
pub fn load_minimap_style(
    mut commands: Commands<'_, '_>,
    root: Option<Res<'_, ResolvedAssetsRoot>>,
    mut status: ResMut<'_, MinimapStyleStatus>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let Some(root) = root else {
        return;
    };
    match MinimapStyle::load(root.as_path()) {
        Ok(style) => commands.insert_resource(style),
        Err(error) => {
            error!("Minimap presentation failed: {error}");
            status.reject(&error);
            exit.send(AppExit::error());
        }
    }
}

/// Creates the minimap image, and its egui texture, at the square's size in physical pixels:
/// once, and again only when that size changes (a window or scale change).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn ensure_minimap_image(
    mut contexts: EguiContexts<'_, '_>,
    style: Option<Res<'_, MinimapStyle>>,
    hud: Res<'_, HudLayout>,
    mut frame: ResMut<'_, MinimapFrame>,
    mut images: ResMut<'_, Assets<Image>>,
) {
    let Some(style) = style else {
        return;
    };
    if !hud.minimap {
        return;
    }
    let Some(pixels_per_point) = contexts.try_ctx_mut().map(|ctx| ctx.pixels_per_point()) else {
        return;
    };
    let side_px = physical_side(style.side_points, pixels_per_point);
    if frame.side_px == side_px && frame.image.is_some() {
        return;
    }
    if let Some(old) = frame.image.take() {
        contexts.remove_image(&old);
    }
    let handle = images.add(minimap_image(side_px));
    frame.texture = Some(contexts.add_image(handle.clone()));
    frame.image = Some(handle);
    frame.side_px = side_px;
    frame.side_points = style.side_points;
    frame.images_created += 1;
}

/// The physical pixel side of a square `side_points` egui points wide.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the product is positive and far below u32::MAX: a window's size in pixels"
)]
fn physical_side(side_points: f32, pixels_per_point: f32) -> u32 {
    (side_points * pixels_per_point).round().max(1.0) as u32
}

/// A `side` by `side` render target the minimap node draws and egui samples.
///
/// The asset keeps its bytes in the main world too (`RenderAssetUsages::default()`), against the
/// plan's render-world-only image (D-1): Bevy removes a render-world-only image from the main
/// world after extraction, and `bevy_egui` 0.33 drops the user texture of every removed image
/// (`free_egui_textures_system`), so egui would show nothing. The bytes are never modified, so
/// the image is extracted once.
fn minimap_image(side: u32) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, u8::MAX],
        MINIMAP_FORMAT,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    image
}

/// Alt+T toggles the terrain, while the world owns the keyboard.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn collect_minimap_keys(
    keyboard: Res<'_, ButtonInput<KeyCode>>,
    ownership: Res<'_, InputOwnership>,
    mut settings: ResMut<'_, MinimapSettings>,
) {
    if !ownership.keyboard || !any_pressed(&keyboard, &ALT_MODIFIERS) {
        return;
    }
    if keyboard.just_pressed(MINIMAP_TERRAIN_KEY) {
        settings.terrain = !settings.terrain;
    }
}

/// Brings the frame up to date: the layout for the current map and image, and the cell texels
/// when the session or the terrain changed.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn prepare_minimap_frame(
    mut frame: ResMut<'_, MinimapFrame>,
    settings: Res<'_, MinimapSettings>,
    session: Res<'_, BattleSession>,
    terrain: Option<Res<'_, TerrainState>>,
    material: Option<Res<'_, TerrainMaterialHandle>>,
    materials: Option<Res<'_, Assets<TerrainMaterial>>>,
    fields: Option<Res<'_, TerrainFieldImages>>,
) {
    let frame = frame.as_mut();
    frame.terrain_on = settings.terrain;
    frame.tick = session.frames.presented().tick.0;
    let Some(terrain) = terrain else {
        frame.layout = None;
        return;
    };
    let size = UVec2::new(terrain.map.width(), terrain.map.height());
    let key = CellsKey {
        generation: session.commands.generation(),
        revision: terrain.revision,
        size,
    };
    if frame.cells_key != Some(key) {
        terrain::write_cell_texels(&terrain.map, &mut frame.cells);
        frame.cells_key = Some(key);
        frame.cells_generation += 1;
        frame.map_size = size;
        frame.albedo = material
            .as_deref()
            .zip(materials.as_deref())
            .and_then(|(handle, materials)| materials.get(&handle.0))
            .map(|material| material.extension.albedo.clone());
        frame.splat = fields.as_deref().map(|fields| fields.splat.clone());
    }
    #[expect(
        clippy::cast_precision_loss,
        reason = "the image side is a window's size in pixels"
    )]
    let side = frame.side_px as f32;
    let layout = MinimapLayout::new(size.x, size.y, side);
    frame.layout = layout;
}
