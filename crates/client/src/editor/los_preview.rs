//! Line-of-sight (LOS) Fog of War interactive preview subsystem.
//!
//! # Architectural Charter
//! In accordance with Implementation Plan 21 §7.5 and [AGENTS.md](file:///home/cl/projects/space-scuffle/AGENTS.md),
//! this module provides dynamic line-of-sight previewing from the currently hovered cursor cell.
//!
//! # Invariants Maintained
//! 1. **Resource Reuse & Zero Allocations:** Reuses a pre-allocated [`PlayerVisionMap`] resource
//!    without reallocating buffers on each tick or cursor movement.
//! 2. **Data-Driven Sight Radius:** Resolves `sight_radius` dynamically by querying the first unit
//!    of the selected loadout from [`UnitCatalog`](sim_core::catalog::UnitCatalog) with zero hardcoded values.
//! 3. **Dirty-Driven Recomputation:** Vision is recalculated strictly when the hovered cell or
//!    the document revision changes.
//! 4. **Overlay B-Channel Binding:** Visibility states are written directly to the terrain overlay
//!    texture's blue channel via [`update_overlay_image`].

use bevy::prelude::*;
use sim_core::catalog::UnitCatalog;
use sim_core::scenario::StartLoadoutCatalog;
use sim_map::vision::{PlayerVisionMap, VisionSource};
use sim_types::{MoverType, SimScalar, SimVec2};

use crate::editor::document::EditorDocument;
use crate::terrain::{
    update_overlay_image, TerrainFieldImages, TerrainParamsState, TerrainState, OVERLAY_FLAG_LOS,
};

/// Newtype resource wrapper around [`StartLoadoutCatalog`] for Bevy ECS insertion.
#[derive(Resource, Debug, Clone, Deref, DerefMut)]
pub struct StartLoadoutCatalogResource(pub StartLoadoutCatalog);

/// Resolves the sight radius for line-of-sight preview dynamically from the unit catalog.
///
/// Looks up the first unit in `loadout_slug` inside `loadouts`, then retrieves its `sight_radius`
/// from `catalog`. Defaults to 9.0 world units if the loadout or catalog entry is absent.
#[must_use]
pub fn resolve_los_preview_sight_radius(
    catalog: &UnitCatalog,
    loadouts: &StartLoadoutCatalog,
    loadout_slug: &str,
) -> SimScalar {
    if let Some(loadout) = loadouts.get(loadout_slug) {
        if let Some(first_unit) = loadout.units.first() {
            if let Some(archetype) = catalog.get_by_slug(&first_unit.slug) {
                return archetype.sight_radius;
            }
        }
    }
    SimScalar::from_num(9)
}

/// ECS Resource managing the state of the line-of-sight preview.
#[derive(Resource)]
pub struct LosPreviewState {
    /// Whether the LOS preview overlay is currently active.
    pub enabled: bool,
    /// Authoritative Fog of War map buffer reused across evaluations.
    pub vision_map: PlayerVisionMap,
    /// Last scout cell coordinate evaluated.
    pub last_scout_cell: Option<(u32, u32)>,
    /// Document revision counter at the time of the last evaluation.
    pub last_revision: u64,
}

impl LosPreviewState {
    /// Creates a new preview state for a map of dimensions `width × height`.
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            enabled: false,
            vision_map: PlayerVisionMap::new(width, height),
            last_scout_cell: None,
            last_revision: u64::MAX,
        }
    }

    /// Resizes the internal vision map buffer if map dimensions have changed.
    pub fn resize(&mut self, width: u32, height: u32) {
        if self.vision_map.width() != width || self.vision_map.height() != height {
            self.vision_map = PlayerVisionMap::new(width, height);
            self.last_scout_cell = None;
            self.last_revision = u64::MAX;
        }
    }
}

/// System updating the LOS preview and synchronizing the overlay image texture.
pub fn update_los_preview_system(
    mut los_state: ResMut<'_, LosPreviewState>,
    doc: Res<'_, EditorDocument>,
    terrain: Res<'_, TerrainState>,
    mut terrain_params: ResMut<'_, TerrainParamsState>,
    field_images: Option<Res<'_, TerrainFieldImages>>,
    mut images: ResMut<'_, Assets<Image>>,
    catalog: Option<Res<'_, crate::CatalogResource>>,
    loadouts: Option<Res<'_, StartLoadoutCatalogResource>>,
    hovered_cell: Option<(u32, u32)>,
    selected_loadout_slug: &str,
) {
    let Some(fields) = field_images else {
        return;
    };
    let Some(overlay_img) = images.get_mut(&fields.overlay) else {
        return;
    };

    los_state.resize(terrain.map.width(), terrain.map.height());

    if !los_state.enabled {
        if terrain_params.overlay_flags & OVERLAY_FLAG_LOS != 0 {
            terrain_params.overlay_flags &= !OVERLAY_FLAG_LOS;
            update_overlay_image(&terrain.map, Some(&doc.validation), None, overlay_img, None);
        }
        return;
    }

    terrain_params.overlay_flags |= OVERLAY_FLAG_LOS;

    let needs_update =
        hovered_cell != los_state.last_scout_cell || doc.revision != los_state.last_revision;

    if !needs_update {
        return;
    }

    los_state.last_scout_cell = hovered_cell;
    los_state.last_revision = doc.revision;

    los_state.vision_map.begin_tick_update();

    if let Some((cx, cy)) = hovered_cell {
        let sight_radius = if let (Some(cat), Some(loads)) = (&catalog, &loadouts) {
            resolve_los_preview_sight_radius(&cat.0, &loads.0, selected_loadout_slug)
        } else {
            SimScalar::from_num(9)
        };

        let pos = SimVec2::new(
            SimScalar::from_num(cx as f32 + 0.5),
            SimScalar::from_num(cy as f32 + 0.5),
        );
        let source = VisionSource {
            pos,
            mover_type: MoverType::Ground,
            sight_radius,
            is_detector: false,
        };

        los_state
            .vision_map
            .apply_vision_source(&terrain.map, &source);
    }

    los_state.vision_map.end_tick_update();

    update_overlay_image(
        &terrain.map,
        Some(&doc.validation),
        Some(&los_state.vision_map),
        overlay_img,
        None,
    );
}
