//! The minimap: a `StarCraft` II style overview of the battle in the bottom left of the HUD
//! (plan `docs/plans/2026-12-01-minimap.md`, B-204).
//!
//! # Responsibility
//! Owns one retained **minimap frame** in the main world ([`MinimapFrame`]): the layout that maps
//! the map onto the image ([`layout`]), the per-cell terrain texels ([`terrain`]), the image the
//! renderer draws into and the egui texture that shows it. The render world draws the frame with
//! one render-graph node and the real shader `assets/shaders/minimap.wgsl` ([`render`]); egui
//! shows the image in the command panel ([`panel`]) and turns presses on it into a
//! [`MinimapIntent`]. The intent moves the camera ([`apply_minimap_intent`]) or, through the
//! battle input's one order path, issues orders at an aim picked on the minimap ([`pick`],
//! [`crate::battle_input::minimap_orders`]). The camera's view is drawn as its ground trapezoid
//! ([`frustum`]). The look is data, `assets/catalog/minimap.ron` ([`style`]).
//!
//! # How it fits
//! ```text
//! Startup             load_minimap_style
//! CollectIntent       ensure_minimap_image, before battle_hud_system (which draws the panel
//!                     and writes MinimapIntent); apply_minimap_intent, then the battle input's
//!                     collect_minimap_orders, after it and before the world's pointer system;
//!                     collect_minimap_keys (Alt+T terrain, Alt+F colours)
//! Present             prepare_minimap_frame, build_minimap_marks, refresh_minimap_fog,
//!                     update_minimap_frustum, after camera_control_system
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

pub mod frustum;
pub mod layout;
pub mod marks;
pub mod panel;
pub mod pick;
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
use crate::battle_pipeline::fog::BattleFog;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_pipeline::BattleSet;
use crate::battle_presentation::BattleProfile;
use crate::battle_selection::BattleSelection;
use crate::input_bindings::{any_pressed, ALT_MODIFIERS, MINIMAP_COLOR_KEY, MINIMAP_TERRAIN_KEY};
use crate::input_policy::InputOwnership;
use crate::player_presentation::PlayerPalette;
use crate::runtime_battle::MapPresentation;
use crate::terrain::{TerrainFieldImages, TerrainMaterial, TerrainMaterialHandle, TerrainState};
use layout::MinimapLayout;
use sim_types::{EntityId, VisibilityState};
use style::{MinimapStyle, MinimapStyleStatus};

/// Format of the minimap image: linear colours written, sRGB stored, so egui, which samples it
/// as linear, shows exactly the colour the shader computed.
pub const MINIMAP_FORMAT: TextureFormat = TextureFormat::Rgba8UnormSrgb;

/// How the minimap colours units and structures (`StarCraft` II's `ColorButton`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ColorMode {
    /// Each owner's player colour.
    #[default]
    Player,
    /// `StarCraft` II's team colours: self, selected, hostile and neutral.
    Team,
}

/// The player's minimap toggles.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapSettings {
    /// Whether the terrain is drawn (`StarCraft` II's `TerrainButton`); off, the map rectangle is
    /// the background and only what is drawn over it remains.
    pub terrain: bool,
    /// Player or team colours.
    pub color_mode: ColorMode,
}

impl Default for MinimapSettings {
    fn default() -> Self {
        Self {
            terrain: true,
            color_mode: ColorMode::Player,
        }
    }
}

/// What the pointer did on the minimap this frame, in world units (simulation `(x, y)`, clamped
/// to the camera's bound; plan `2026-12-01-minimap` D-7). Written by the HUD each frame it draws
/// the minimap, cleared at the start of every HUD frame.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct MinimapIntent {
    /// Left button pressed or dragged with no command mode armed: centre the camera here.
    pub look_at: Option<Vec2>,
    /// Left press with a command mode armed: aim the mode here.
    pub aim_left: Option<Vec2>,
    /// Right press: the smart command here, or cancel an armed mode.
    pub aim_right: Option<Vec2>,
    /// Shift held at the press.
    pub shift: bool,
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
    subjects: Vec<marks::MarkSubject>,
    marks_generation: u64,
    marks_key: Option<MarksKey>,
    selection_seen: Vec<EntityId>,
    fog: Vec<u8>,
    fog_generation: u64,
    fog_key: Option<(u64, UVec2)>,
    terrain_on: bool,
    tick: u32,
    frustum: Option<[Vec2; 4]>,
}

/// What the marks were built from, besides the selection.
#[derive(Debug, Clone, Copy, PartialEq)]
struct MarksKey {
    generation: Option<SessionGeneration>,
    tick: u32,
    refills: u64,
    map_tick: Option<u32>,
    mode: ColorMode,
    layout: MinimapLayout,
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

    /// What each mark shows, index for index with [`Self::marks`].
    #[must_use]
    pub fn subjects(&self) -> &[marks::MarkSubject] {
        &self.subjects
    }

    /// The camera's ground trapezoid this frame, in world units: top-left, top-right,
    /// bottom-right, bottom-left of the view ([`frustum`]).
    #[must_use]
    pub const fn frustum(&self) -> Option<[Vec2; 4]> {
        self.frustum
    }

    /// Bumped by every rebuild of [`Self::marks`].
    #[must_use]
    pub const fn marks_generation(&self) -> u64 {
        self.marks_generation
    }

    /// The fog bytes, one per cell row-major: how far each cell darkens toward the background.
    #[must_use]
    pub fn fog(&self) -> &[u8] {
        &self.fog
    }

    /// Bumped by every rebuild of [`Self::fog`].
    #[must_use]
    pub const fn fog_generation(&self) -> u64 {
        self.fog_generation
    }

    /// A frame for an image of `side_px` pixels and `side_points` points that no egui sizes: for
    /// apps without a window, such as the allocation tests, which run the frame's systems alone.
    #[doc(hidden)]
    #[must_use]
    pub fn with_image_side(side_px: u32, side_points: f32) -> Self {
        Self {
            side_points,
            side_px,
            ..Self::default()
        }
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
            (
                "minimap.subjects",
                self.subjects.as_ptr().addr(),
                self.subjects.capacity(),
            ),
            (
                "minimap.selection_seen",
                self.selection_seen.as_ptr().addr(),
                self.selection_seen.capacity(),
            ),
            ("minimap.fog", self.fog.as_ptr().addr(), self.fog.capacity()),
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
            .init_resource::<MinimapIntent>()
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
                    (
                        apply_minimap_intent,
                        crate::battle_input::minimap_orders::collect_minimap_orders,
                    )
                        .chain()
                        .after(crate::battle_hud::panels::battle_hud_system)
                        .after(crate::input_policy::sample_input_ownership)
                        .after(crate::lab_ui_system)
                        .before(crate::battle_input::collect_pointer_intent)
                        .in_set(BattleSet::CollectIntent),
                    (
                        prepare_minimap_frame,
                        build_minimap_marks,
                        refresh_minimap_fog,
                        update_minimap_frustum,
                    )
                        .chain()
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

/// Alt+T toggles the terrain and Alt+F the colour mode, while the world owns the keyboard.
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
    if keyboard.just_pressed(MINIMAP_COLOR_KEY) {
        settings.color_mode = match settings.color_mode {
            ColorMode::Player => ColorMode::Team,
            ColorMode::Team => ColorMode::Player,
        };
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

/// What the marks builder reads besides the frame.
#[derive(bevy::ecs::system::SystemParam)]
pub struct MarkInputs<'w> {
    settings: Res<'w, MinimapSettings>,
    style: Option<Res<'w, MinimapStyle>>,
    session: Res<'w, BattleSession>,
    fog: Option<Res<'w, BattleFog>>,
    palette: Option<Res<'w, PlayerPalette>>,
    objects: Option<Res<'w, MapPresentation>>,
    terrain: Option<Res<'w, TerrainState>>,
    selection: Res<'w, BattleSelection>,
}

/// Rebuilds the marks when the presented tick, the fog, the map's billboards, the selection, the
/// colour mode or the layout changed, and never otherwise (plan `2026-12-01-minimap` D-5).
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn build_minimap_marks(mut frame: ResMut<'_, MinimapFrame>, inputs: MarkInputs<'_>) {
    let frame = frame.as_mut();
    let (Some(layout), Some(style)) = (frame.layout, inputs.style.as_deref()) else {
        return;
    };
    let fog = inputs.fog.as_deref();
    let key = MarksKey {
        generation: inputs.session.commands.generation(),
        tick: inputs.session.frames.presented().tick.0,
        refills: fog.map_or(0, BattleFog::refills),
        map_tick: inputs.objects.as_deref().and_then(|objects| objects.tick),
        mode: inputs.settings.color_mode,
        layout,
    };
    let selection = inputs.selection.ids();
    if frame.marks_key == Some(key) && frame.selection_seen.as_slice() == selection {
        return;
    }
    frame.selection_seen.clear();
    frame.selection_seen.extend_from_slice(selection);
    let sources = marks::MarkSources {
        layout,
        style,
        mode: key.mode,
        viewer: fog.and_then(BattleFog::viewer),
        fog,
        bubbles: &inputs.session.frames.presented().bubbles,
        catalog: inputs.session.world().map(|world| &world.catalog),
        palette: inputs.palette.as_deref(),
        objects: inputs
            .objects
            .as_deref()
            .map_or(&[], |objects| objects.objects.as_slice()),
        nodes: inputs
            .terrain
            .as_deref()
            .map_or(&[], |terrain| terrain.map.resources()),
        selected: selection,
    };
    marks::build_marks(&sources, &mut frame.marks, &mut frame.subjects);
    frame.marks_key = Some(key);
    frame.marks_generation += 1;
}

/// Rebuilds the fog bytes when the fog was refilled or the map changed size (plan
/// `2026-12-01-minimap` D-4). A fog with no cells for this map reads as unexplored, so the
/// minimap fails closed.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_minimap_fog(
    mut frame: ResMut<'_, MinimapFrame>,
    style: Option<Res<'_, MinimapStyle>>,
    fog: Option<Res<'_, BattleFog>>,
) {
    let frame = frame.as_mut();
    let (Some(style), Some(fog)) = (style.as_deref(), fog.as_deref()) else {
        return;
    };
    let size = frame.map_size;
    let key = (fog.refills(), size);
    if frame.fog_key == Some(key) {
        return;
    }
    let darkening = style.fog_darkening;
    let byte = |state: VisibilityState| {
        let level = match state {
            VisibilityState::Visible => darkening.visible,
            VisibilityState::Fogged => darkening.fogged,
            VisibilityState::Unexplored => darkening.unexplored,
        };
        unit_byte(level)
    };
    let cells = size.x as usize * size.y as usize;
    let states = fog.cell_states();
    frame.fog.clear();
    if states.len() == cells {
        frame.fog.extend(states.iter().map(|state| byte(*state)));
    } else {
        frame.fog.resize(cells, byte(VisibilityState::Unexplored));
    }
    frame.fog_key = Some(key);
    frame.fog_generation += 1;
}

/// Centres the camera where the minimap's left button points (plan `2026-12-01-minimap` D-7).
/// `camera_control_system` clamps the target and eases the height as for any move. A jump
/// cancels a middle-button camera drag in progress, so the two never fight.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn apply_minimap_intent(
    intent: Res<'_, MinimapIntent>,
    mut cameras: Query<'_, '_, &mut crate::RtsCamera>,
    mut drag: ResMut<'_, crate::camera_drag::CameraDragState>,
) {
    let Some(target) = intent.look_at else {
        return;
    };
    for mut camera in &mut cameras {
        camera.target = target;
    }
    drag.interrupt();
}

/// Casts the camera's view onto the ground it looks at, every frame (plan `2026-12-01-minimap`
/// D-8). The pose is the one `camera_control_system` just computed, so the trapezoid matches the
/// frame the world draws.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn update_minimap_frustum(
    mut frame: ResMut<'_, MinimapFrame>,
    style: Option<Res<'_, MinimapStyle>>,
    cameras: Query<'_, '_, (&Camera, &Transform, &crate::RtsCamera)>,
) {
    let frame = frame.as_mut();
    frame.frustum = style.as_deref().and_then(|style| {
        let (camera, transform, rts) = cameras.get_single().ok()?;
        frustum::ground_trapezoid(
            camera,
            &GlobalTransform::from(*transform),
            rts.look_at_y,
            style.frustum_max_distance,
        )
    });
}

/// `value` in `[0, 1]` as the nearest byte.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to [0, 1] first, so the product is in [0, 255]"
)]
fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * f32::from(u8::MAX)).round() as u8
}
