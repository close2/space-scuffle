//! Viewport mouse interaction: picking, drag gestures and the live brush overlay.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.3, this module turns pointer input over the
//! terrain into document edits using the pure tool logic in [`crate::editor::tools`].
//!
//! # Invariants Maintained
//! 1. **Panels Own Their Pixels:** Pointer ownership is read after the panels are drawn, so input
//!    above a panel never reaches the terrain behind it.
//! 2. **Drags Always End:** A drag finishes wherever its button is released. Strokes keep their undo
//!    entry; drags that change nothing until release are cancelled without an end cell.
//! 3. **Renderer Follows the Document:** Every cell edit reaches [`TerrainState`], including the
//!    mirrored half of a symmetric edit.

use bevy::prelude::*;
use bevy_egui::egui;
use sim_map::map_format::MapDocument;
use sim_map::validation::CellRect;

use super::{
    find_entity_by_id, EditorTool, EditorToolState, EditorUiContext, EditorUiState,
    EditorUiTerrain, EntityDrag, RampToolMode,
};
use crate::editor::commands::{ChangeSet, History};
use crate::editor::document::EditorDocument;
use crate::editor::symmetry::SymmetryMode;
use crate::editor::tools::{
    box_fill_command, check_ramp_placement, erase_cell_command, erase_entities_command, infer_ramp,
    move_entity_command, paint_brush_stamp, pick_entity, place_entity_command, place_ramp_command,
    BrushShape,
};
use crate::terrain::{pick_terrain, TerrainParamsState, TerrainState, OVERLAY_FLAG_BRUSH};

/// Runs the active tool if the pointer is over the terrain, then ends any released drag.
pub(super) fn update_viewport(
    ctx: &egui::Context,
    input: &EditorUiContext<'_, '_>,
    state: &mut EditorUiState<'_>,
    terrain: &mut EditorUiTerrain<'_>,
    now_secs: f64,
) {
    let EditorUiState {
        doc,
        history,
        tool_state,
        ..
    } = state;
    // egui only knows what the panels cover once they have been shown this frame, so asking
    // earlier lets clicks and drag releases fall through to the terrain behind them.
    if ctx.wants_pointer_input() || ctx.is_pointer_over_area() {
        hide_brush_overlay(&mut terrain.terrain_params);
        tool_state.hovered_cell = None;
    } else {
        handle_viewport_tool_interaction(
            &input.mouse_buttons,
            &input.windows,
            &input.camera_query,
            doc,
            history,
            tool_state,
            &mut terrain.terrain_state,
            &mut terrain.terrain_params,
            now_secs,
        );
    }
    finish_released_gestures(&input.mouse_buttons, history, tool_state);
}

/// Ends any drag whose button is no longer held, wherever the pointer ended up.
///
/// The viewport tools only run while the pointer is over terrain, so a release above a panel,
/// off the map or outside the window never reaches them. Recording here guarantees that a stroke
/// already applied to the document always gets its undo entry. Box, ramp and entity drags change
/// nothing until released over the terrain, so without an end cell they are cancelled.
fn finish_released_gestures(
    mouse_buttons: &ButtonInput<MouseButton>,
    history: &mut History,
    tool_state: &mut EditorToolState,
) {
    if mouse_buttons.pressed(MouseButton::Left) {
        return;
    }
    if tool_state.is_painting {
        tool_state.is_painting = false;
        if let Some(cmd) = tool_state.stroke_recorder.finish("Paint Brush") {
            history.push_applied(cmd);
        }
    }
    tool_state.box_start_cell = None;
    tool_state.ramp_start_cell = None;
    tool_state.entity_drag = None;
}

/// Hides the brush outline, touching the parameters only if it is showing.
fn hide_brush_overlay(terrain_params: &mut ResMut<'_, TerrainParamsState>) {
    if terrain_params.overlay_flags & OVERLAY_FLAG_BRUSH != 0 {
        terrain_params.overlay_flags &= !OVERLAY_FLAG_BRUSH;
    }
}

/// Shows the brush outline over `cell`, touching the parameters only if something moved.
#[expect(
    clippy::cast_precision_loss,
    reason = "map cell coordinates stay far below the range of f32 and i32"
)]
fn show_brush_overlay(
    terrain_params: &mut ResMut<'_, TerrainParamsState>,
    cell: (u32, u32),
    radius: u8,
    shape: BrushShape,
) {
    let center = Vec2::new(cell.0 as f32 + 0.5, cell.1 as f32 + 0.5);
    let radius = f32::from(radius);
    let shape = match shape {
        BrushShape::Circle => 0,
        BrushShape::Square => 1,
    };
    if terrain_params.brush_center != center
        || terrain_params.brush_radius.to_bits() != radius.to_bits()
        || terrain_params.brush_shape != shape
        || terrain_params.overlay_flags & OVERLAY_FLAG_BRUSH == 0
    {
        terrain_params.brush_center = center;
        terrain_params.brush_radius = radius;
        terrain_params.brush_shape = shape;
        terrain_params.overlay_flags |= OVERLAY_FLAG_BRUSH;
    }
}

// PERFORMANCE RATIONALE:
// Why: A plain `&mut EditorDocument` argument is taken through `DerefMut`, which flags the resource
// changed on every frame and made every marker rebuild and the terrain material re-upload per frame.
// What: These helpers borrow the `ResMut` wrappers and dereference mutably only on a real edit.
// Trade-off: Slightly noisier signatures in exchange for idle frames that do no rebuild work.
#[expect(
    clippy::too_many_arguments,
    reason = "each Bevy system parameter is a separate declared data dependency"
)]
fn handle_viewport_tool_interaction(
    mouse_buttons: &ButtonInput<MouseButton>,
    windows: &Query<'_, '_, &Window>,
    camera_query: &Query<'_, '_, (&Camera, &GlobalTransform, &mut crate::camera::RtsCamera)>,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    terrain: &mut ResMut<'_, TerrainState>,
    terrain_params: &mut ResMut<'_, TerrainParamsState>,
    now_secs: f64,
) {
    let Ok(window) = windows.get_single() else {
        return;
    };
    let Some(cursor_pos) = window.cursor_position() else {
        hide_brush_overlay(terrain_params);
        tool_state.hovered_cell = None;
        return;
    };
    let Ok((camera, camera_transform, _)) = camera_query.get_single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(camera_transform, cursor_pos) else {
        return;
    };

    let hit = pick_terrain(&terrain.map, ray.origin, *ray.direction);

    // Update brush hover overlay
    let hit_cell = if let Some(ref h) = hit {
        tool_state.hovered_cell = Some(h.cell);
        show_brush_overlay(
            terrain_params,
            h.cell,
            tool_state.brush_radius,
            tool_state.brush_shape,
        );
        h.cell
    } else {
        tool_state.hovered_cell = None;
        hide_brush_overlay(terrain_params);
        return;
    };

    // Viewport mouse interactions based on active tool
    match tool_state.current_tool {
        EditorTool::Brush => {
            run_brush_tool(mouse_buttons, doc, tool_state, terrain, hit_cell, now_secs);
        }
        EditorTool::Box => run_box_tool(
            mouse_buttons,
            doc,
            history,
            tool_state,
            terrain,
            hit_cell,
            now_secs,
        ),
        EditorTool::Ramp => run_ramp_tool(
            mouse_buttons,
            doc,
            history,
            tool_state,
            terrain,
            hit_cell,
            now_secs,
        ),
        EditorTool::Entity => {
            run_entity_tool(mouse_buttons, doc, history, tool_state, hit_cell, now_secs);
        }
        EditorTool::Select => {
            run_select_tool(mouse_buttons, doc, history, tool_state, hit_cell, now_secs);
        }
        EditorTool::Eraser => run_eraser_tool(
            mouse_buttons,
            doc,
            history,
            tool_state,
            terrain,
            hit_cell,
            now_secs,
        ),
    }
}

fn run_brush_tool(
    mouse_buttons: &ButtonInput<MouseButton>,
    doc: &mut ResMut<'_, EditorDocument>,
    tool_state: &mut EditorToolState,
    terrain: &mut ResMut<'_, TerrainState>,
    hit_cell: (u32, u32),
    now_secs: f64,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        tool_state.is_painting = true;
        tool_state.stroke_recorder.begin();
        paint_brush_stamp(
            &mut tool_state.stroke_recorder,
            &mut doc.doc,
            tool_state.brush_shape,
            tool_state.brush_radius,
            hit_cell,
            &tool_state.paint_op,
            tool_state.symmetry,
            &mut tool_state.scratch_coords,
        );
        sync_stamp_to_terrain(
            &tool_state.scratch_coords,
            tool_state.symmetry,
            doc,
            terrain,
            now_secs,
        );
    } else if mouse_buttons.pressed(MouseButton::Left) && tool_state.is_painting {
        paint_brush_stamp(
            &mut tool_state.stroke_recorder,
            &mut doc.doc,
            tool_state.brush_shape,
            tool_state.brush_radius,
            hit_cell,
            &tool_state.paint_op,
            tool_state.symmetry,
            &mut tool_state.scratch_coords,
        );
        sync_stamp_to_terrain(
            &tool_state.scratch_coords,
            tool_state.symmetry,
            doc,
            terrain,
            now_secs,
        );
    }
}

fn run_box_tool(
    mouse_buttons: &ButtonInput<MouseButton>,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    terrain: &mut ResMut<'_, TerrainState>,
    hit_cell: (u32, u32),
    now_secs: f64,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        tool_state.box_start_cell = Some(hit_cell);
    } else if mouse_buttons.just_released(MouseButton::Left) {
        if let Some(start_cell) = tool_state.box_start_cell.take() {
            let rect = CellRect {
                min: (start_cell.0.min(hit_cell.0), start_cell.1.min(hit_cell.1)),
                max_inclusive: (start_cell.0.max(hit_cell.0), start_cell.1.max(hit_cell.1)),
            };
            let cmd = box_fill_command(
                &doc.doc,
                rect,
                &tool_state.paint_op,
                tool_state.symmetry,
                "Box Fill",
            );
            let mut cs = ChangeSet::default();
            history.execute(cmd, &mut doc.doc, &mut cs);
            doc.apply_changeset(&cs, now_secs);
            doc.sync_terrain(&cs, terrain);
        }
    }
}

fn run_ramp_tool(
    mouse_buttons: &ButtonInput<MouseButton>,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    terrain: &mut ResMut<'_, TerrainState>,
    hit_cell: (u32, u32),
    now_secs: f64,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        tool_state.ramp_start_cell = Some(hit_cell);
    } else if mouse_buttons.just_released(MouseButton::Left) {
        if let Some(start_cell) = tool_state.ramp_start_cell.take() {
            let rect = CellRect {
                min: (start_cell.0.min(hit_cell.0), start_cell.1.min(hit_cell.1)),
                max_inclusive: (start_cell.0.max(hit_cell.0), start_cell.1.max(hit_cell.1)),
            };
            place_ramp_rect(rect, doc, history, tool_state, terrain, now_secs);
        }
    }
}

fn run_entity_tool(
    mouse_buttons: &ButtonInput<MouseButton>,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    hit_cell: (u32, u32),
    now_secs: f64,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        let cmd = place_entity_command(
            &doc.doc,
            tool_state.entity_kind,
            hit_cell,
            tool_state.symmetry,
        );
        let mut cs = ChangeSet::default();
        history.execute(cmd, &mut doc.doc, &mut cs);
        doc.apply_changeset(&cs, now_secs);
        tool_state.last_status_message = Some("Placed entity".into());
    }
}

fn run_select_tool(
    mouse_buttons: &ButtonInput<MouseButton>,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    hit_cell: (u32, u32),
    now_secs: f64,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        let picked = pick_entity(&doc.doc, hit_cell).map(|entity| entity.id());
        tool_state.selected_entity_id = picked;
        tool_state.entity_drag = picked.map(|id| EntityDrag {
            id,
            grab_cell: hit_cell,
        });
    } else if mouse_buttons.just_released(MouseButton::Left) {
        if let Some(drag) = tool_state.entity_drag.take() {
            move_dragged_entity(drag, hit_cell, doc, history, tool_state, now_secs);
        }
    }
}

fn run_eraser_tool(
    mouse_buttons: &ButtonInput<MouseButton>,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    terrain: &mut ResMut<'_, TerrainState>,
    hit_cell: (u32, u32),
    now_secs: f64,
) {
    if mouse_buttons.just_pressed(MouseButton::Left) {
        if let Some(cmd) = erase_entities_command(&doc.doc, &[hit_cell], tool_state.symmetry) {
            let mut cs = ChangeSet::default();
            history.execute(cmd, &mut doc.doc, &mut cs);
            doc.apply_changeset(&cs, now_secs);
            tool_state.last_status_message = Some("Erased entity".into());
        } else if let Some(cmd) = erase_cell_command(&doc.doc, hit_cell, tool_state.symmetry) {
            let mut cs = ChangeSet::default();
            history.execute(cmd, &mut doc.doc, &mut cs);
            doc.apply_changeset(&cs, now_secs);
            doc.sync_terrain(&cs, terrain);
        }
    }
}

/// Validates and applies a ramp over `rect`, reporting a rejected placement on the status bar.
fn place_ramp_rect(
    rect: CellRect,
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    terrain: &mut ResMut<'_, TerrainState>,
    now_secs: f64,
) {
    let placement = match tool_state.ramp_mode {
        RampToolMode::Auto => infer_ramp(&doc.doc, rect),
        RampToolMode::Manual(dir) => check_ramp_placement(&doc.doc, rect, dir),
    };
    let command = placement.and_then(|placement| {
        place_ramp_command(&doc.doc, rect, placement, tool_state.symmetry, "Place Ramp")
    });
    match command {
        Ok(cmd) => {
            let mut cs = ChangeSet::default();
            history.execute(cmd, &mut doc.doc, &mut cs);
            doc.apply_changeset(&cs, now_secs);
            doc.sync_terrain(&cs, terrain);
            tool_state.last_status_message = Some("Placed ramp".into());
        }
        Err(e) => {
            tool_state.last_status_message = Some(format!("Ramp error: {e}"));
        }
    }
}

/// Moves the dragged entity by the distance between its grab cell and `drop_cell`.
///
/// A multi-cell base keeps its position under the pointer wherever it was grabbed. Dropping on
/// the grab cell is a plain click, which leaves no undo entry and no unsaved changes.
fn move_dragged_entity(
    drag: EntityDrag,
    drop_cell: (u32, u32),
    doc: &mut ResMut<'_, EditorDocument>,
    history: &mut History,
    tool_state: &mut EditorToolState,
    now_secs: f64,
) {
    if drop_cell == drag.grab_cell {
        return;
    }
    let Some(entity) = find_entity_by_id(&doc.doc, drag.id) else {
        return;
    };
    let anchor = entity.cell();
    let new_cell = (
        (anchor.0 + drop_cell.0).saturating_sub(drag.grab_cell.0),
        (anchor.1 + drop_cell.1).saturating_sub(drag.grab_cell.1),
    );
    let cmd = move_entity_command(&doc.doc, &entity, new_cell, tool_state.symmetry);
    let mut cs = ChangeSet::default();
    history.execute(cmd, &mut doc.doc, &mut cs);
    doc.apply_changeset(&cs, now_secs);
    tool_state.last_status_message = Some("Moved entity".into());
}

/// Copies a brush stamp from the document into the terrain, including its mirrored footprint.
///
/// `cells` holds only the primary footprint; [`paint_brush_stamp`] also painted its reflection.
// PERFORMANCE RATIONALE:
// Why: A single bounding box around a stamp and its mirror spans most of the map, and copying it
// on every drag frame is O(W × H).
// What: The primary and mirrored rectangles are copied separately, so the work stays O(brush).
// Trade-off: Two terrain writes per stamp. `TerrainState` still unions them when choosing dirty
// chunks, so the throttled mesh rebuild covers the band between them.
fn sync_stamp_to_terrain(
    cells: &[(u32, u32)],
    symmetry: SymmetryMode,
    doc: &mut EditorDocument,
    terrain: &mut TerrainState,
    now_secs: f64,
) {
    let Some(&first) = cells.first() else {
        return;
    };
    let mut rect = CellRect {
        min: first,
        max_inclusive: first,
    };
    for &(x, y) in cells {
        rect.min = (rect.min.0.min(x), rect.min.1.min(y));
        rect.max_inclusive = (rect.max_inclusive.0.max(x), rect.max_inclusive.1.max(y));
    }

    let copy_rect = |rect: CellRect, terrain: &mut TerrainState| {
        terrain.apply_cells(rect, |x, y, target_cell| {
            if let Some(spec) = doc.doc.cell(x, y) {
                *target_cell = MapDocument::to_map_cell(spec);
            }
        });
    };
    copy_rect(rect, terrain);
    if symmetry != SymmetryMode::None {
        copy_rect(
            symmetry.mirror_rect(rect, doc.doc.width, doc.doc.height),
            terrain,
        );
    }

    doc.revision = doc.revision.wrapping_add(1);
    doc.dirty = true;
    doc.last_edit_at_secs = now_secs;
}
