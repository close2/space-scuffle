//! Checks prepared identity against actual shell/indicator/effect material data
//! and real propagated geometry. Pixel bounds use the same viewport origin/DPI
//! conversion as production picking and the existing capture harness.

use super::super::config::CaptureError;
use super::{failure, EffectProof, OwnershipState, View};
use client::player_presentation::PlayerPalette;
use client::vfx::{CombatEffectOwner, CombatFxMaterial};
use bevy::prelude::*;
use bevy::render::primitives::Aabb;
use serde::Serialize;

/// Actual visible shell identity, prepared linear tint, and physical-window projection.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runtime_capture) struct BubbleProof {
    pub entity: u32,
    pub owner: u8,
    pub linear_rgb: [f32; 3],
    pub ring_rgb: Vec<[f32; 3]>,
    pub pixel: Option<[f32; 2]>,
}

/// Committed participant label and its prepared sRGB UI color.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runtime_capture) struct LabelProof {
    pub owner: u8,
    pub label: String,
    pub srgb_u8: [u8; 3],
}

/// Actual mesh bounds in physical-window pixels and captured material identity.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runtime_capture) struct GeometryProof {
    pub entity: u64,
    pub minimum: [f32; 2],
    pub maximum: [f32; 2],
    pub shader_kind: Option<u32>,
    pub owner: u8,
    pub linear_rgb: [f32; 3],
}

/// One screenshot checkpoint’s actual entity, material, label and lifetime evidence.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runtime_capture) struct OwnershipProof {
    pub labels: Vec<LabelProof>,
    pub bubbles: Vec<BubbleProof>,
    pub effects: Vec<EffectProof>,
    pub geometry: Vec<GeometryProof>,
    pub preview_effect: Option<String>,
    pub obstacles: Vec<ObstacleProof>,
    pub minimum_separation_pixels: Option<f32>,
    pub frozen: super::freeze::Fingerprint,
    pub stage_origin: Option<[f32; 3]>,
    pub stage_focus: Option<[f32; 2]>,
}

/// Actual shell rectangle used to reject an occluded demonstration stage.
#[derive(Debug, Clone, Serialize)]
pub(in crate::runtime_capture) struct ObstacleProof {
    pub entity: u64,
    pub kind: &'static str,
    pub minimum: [f32; 2],
    pub maximum: [f32; 2],
}

/// Euclidean distance between two closed axis-aligned pixel rectangles.
fn rectangle_gap(a: (Vec2, Vec2), b: (Vec2, Vec2)) -> f32 {
    (a.0 - b.1).max(b.0 - a.1).max(Vec2::ZERO).length()
}

struct ProjectionView {
    camera: Camera,
    transform: GlobalTransform,
    region: [f32; 4],
    resolution: [u32; 2],
}

impl ProjectionView {
    fn load(
        world: &mut World,
        region: [f32; 4],
        resolution: [u32; 2],
    ) -> Result<Self, CaptureError> {
        let mut query = world.query_filtered::<(&Camera, &GlobalTransform), With<Camera3d>>();
        let (camera, transform) = query
            .get_single(world)
            .map_err(|error| failure(error.to_string()))?;
        Ok(Self {
            camera: camera.clone(),
            transform: *transform,
            region,
            resolution,
        })
    }
    fn project(&self, point: Vec3) -> Option<Vec2> {
        let logical = client::input_policy::world_to_window(&self.camera, &self.transform, point)?;
        Some(
            client::input_policy::ViewportCoordinates::from_camera(&self.camera)?
                .physical_from_window(logical),
        )
    }
    fn inside(&self, point: Vec2) -> bool {
        let [x, y, w, h] = self.region;
        let size = Vec2::new(self.resolution[0] as f32, self.resolution[1] as f32);
        point.cmpge(Vec2::new(x, y) * size).all()
            && point.cmple(Vec2::new(x + w, y + h) * size).all()
    }
    fn rectangle(
        &self,
        aabb: &Aabb,
        global: &GlobalTransform,
    ) -> Result<(Vec2, Vec2), CaptureError> {
        if !self.transform.compute_matrix().is_finite() || !self.camera.clip_from_view().is_finite()
        {
            return Err(failure("nonfinite ownership projection context"));
        }
        let mut low = Vec2::splat(f32::INFINITY);
        let mut high = Vec2::splat(f32::NEG_INFINITY);
        for corner in super::staging::corners(aabb, global)? {
            let point = self.project(corner).ok_or_else(|| {
                failure("ownership bounds corner clipped or projection unavailable")
            })?;
            if !point.is_finite() {
                return Err(failure("ownership bounds projection is nonfinite"));
            }
            low = low.min(point);
            high = high.max(point);
        }
        Ok((low, high))
    }
    fn bounds(
        &self,
        aabb: &Aabb,
        global: &GlobalTransform,
        entity: Entity,
        kind: Option<u32>,
        identity: (u8, [f32; 3]),
    ) -> Result<Option<GeometryProof>, CaptureError> {
        let (low, high) = self.rectangle(aabb, global)?;
        if !self.inside(low) || !self.inside(high) || (high - low).min_element() < 0.5 {
            return Ok(None);
        }
        Ok(Some(GeometryProof {
            entity: entity.to_bits(),
            minimum: low.to_array(),
            maximum: high.to_array(),
            shader_kind: kind,
            owner: identity.0,
            linear_rgb: identity.1,
        }))
    }
}

fn bubble_proofs(
    world: &mut World,
    view: &ProjectionView,
) -> Result<Vec<BubbleProof>, CaptureError> {
    let mut query = world.query::<(
        &client::BubbleInstance,
        &MeshMaterial3d<client::BubbleDomeMaterial>,
        &GlobalTransform,
        &ViewVisibility,
        Option<&Children>,
    )>();
    let palette = world.resource::<PlayerPalette>();
    let materials = world.resource::<Assets<client::BubbleDomeMaterial>>();
    let rings = world.resource::<Assets<client::SelectionRingMaterial>>();
    let mut output = Vec::new();
    for (bubble, handle, global, visibility, children) in query.iter(world) {
        if !visibility.get() {
            continue;
        }
        let style = palette
            .resolve(bubble.0.owner)
            .map_err(|error| failure(error.to_string()))?;
        let material = materials
            .get(&handle.0)
            .ok_or_else(|| failure("ownership shell material missing"))?;
        if material.uniform.team_color != style.linear {
            return Err(failure("shell hue differs from exact owner palette"));
        }
        let mut ring_rgb = Vec::new();
        if let Some(children) = children {
            for child in children {
                if let Some(handle) =
                    world.get::<MeshMaterial3d<client::SelectionRingMaterial>>(*child)
                {
                    let ring = rings
                        .get(&handle.0)
                        .ok_or_else(|| failure("ownership ring material missing"))?;
                    if ring.uniform.team_color != style.linear {
                        return Err(failure("retained ring hue differs from shell owner"));
                    }
                    ring_rgb.push(ring.uniform.team_color.truncate().to_array());
                }
            }
        }
        output.push(BubbleProof {
            entity: bubble.0.entity_id.0,
            owner: bubble.0.owner.0,
            linear_rgb: style.linear.truncate().to_array(),
            ring_rgb,
            pixel: view
                .project(global.translation())
                .filter(|point| view.inside(*point))
                .map(|point| point.to_array()),
        });
    }
    Ok(output)
}

fn tower_proof(
    world: &mut World,
    view: &ProjectionView,
) -> Result<Option<GeometryProof>, CaptureError> {
    let mut query = world.query_filtered::<(
        Entity,
        &GlobalTransform,
        &ViewVisibility,
        &MeshMaterial3d<StandardMaterial>,
        Option<&Aabb>,
    ), With<client::runtime_battle::MapObject>>();
    let record = world
        .resource::<client::runtime_battle::MapPresentation>()
        .objects
        .iter()
        .find(|record| record.ownership_tint && record.owner.is_none())
        .ok_or_else(|| failure("neutral tower disappeared"))?;
    let expected = world
        .resource::<PlayerPalette>()
        .resolve(sim_types::PlayerId::NEUTRAL)
        .map_err(|error| failure(error.to_string()))?
        .color();
    for (entity, global, visible, handle, bounds) in query.iter(world) {
        if global.translation().distance(record.position) > 0.0001 {
            continue;
        }
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&handle.0)
            .ok_or_else(|| failure("tower material missing"))?;
        if material.base_color.to_linear() != expected.to_linear() {
            return Err(failure("neutral tower hue is not authored neutral"));
        }
        if !visible.get() {
            return Err(failure("neutral tower is not visible"));
        }
        let Some(bounds) = bounds else {
            return Ok(None);
        };
        return view
            .bounds(
                bounds,
                global,
                entity,
                None,
                (
                    sim_types::PlayerId::NEUTRAL.0,
                    Vec4::from_array(expected.to_linear().to_f32_array())
                        .truncate()
                        .to_array(),
                ),
            )?
            .map(Some)
            .ok_or_else(|| failure("neutral tower geometry outside gameplay ROI"));
    }
    Err(failure("neutral tower has no actual mesh"))
}

fn effect_geometry(
    world: &mut World,
    view: &ProjectionView,
    kind: super::EffectKind,
) -> Result<Option<Vec<GeometryProof>>, CaptureError> {
    let mut query = world.query::<(
        Entity,
        &CombatEffectOwner,
        &MeshMaterial3d<CombatFxMaterial>,
        &GlobalTransform,
        &ViewVisibility,
        Option<&Aabb>,
    )>();
    let owner = world.resource::<PlayerPalette>().effect();
    let materials = world.resource::<Assets<CombatFxMaterial>>();
    let mut output = Vec::new();
    let mut count = 0;
    for (entity, captured, handle, global, visible, bounds) in query.iter(world) {
        let material = materials
            .get(&handle.0)
            .ok_or_else(|| failure("preview effect material missing"))?;
        if captured.0 != owner || material.uniform.fx_kind != kind.shader_kind() {
            return Err(failure("preview contains stale/wrong-owner effect"));
        }
        count += 1;
        if material.uniform.age <= 0.0 {
            return Ok(None);
        }
        let Some(bounds) = bounds else {
            return Ok(None);
        };
        if visible.get() {
            if let Some(proof) = view.bounds(
                bounds,
                global,
                entity,
                Some(kind.shader_kind()),
                (
                    captured.0 .0,
                    material.uniform.primary_color.truncate().to_array(),
                ),
            )? {
                output.push(proof);
            }
        }
    }
    let expected = if kind == super::EffectKind::Shatter {
        17
    } else {
        1
    };
    if count != expected || output.len() != expected {
        return Err(failure(format!(
            "preview {kind:?} requires {expected} actual visible geometries, found {count}/{}",
            output.len()
        )));
    }
    Ok(Some(output))
}

fn obstacles(
    world: &mut World,
    view: &ProjectionView,
) -> Result<Option<Vec<ObstacleProof>>, CaptureError> {
    let mut query = world.query_filtered::<(
        Entity,
        Option<&Aabb>,
        Option<&GlobalTransform>,
        Option<&ViewVisibility>,
    ), With<client::LabPreviewBubble>>();
    let mut output = Vec::new();
    for (entity, bounds, global, visible) in query.iter(world) {
        let Some(visible) = visible else {
            return Ok(None);
        };
        if !visible.get() {
            continue;
        }
        let (Some(bounds), Some(global)) = (bounds, global) else {
            return Ok(None);
        };
        let (minimum, maximum) = view.rectangle(bounds, global)?;
        output.push(ObstacleProof {
            entity: entity.to_bits(),
            kind: "shell",
            minimum: minimum.to_array(),
            maximum: maximum.to_array(),
        });
    }
    if !output.iter().any(|item| item.kind == "shell") {
        return Err(failure("foreground stage has no visible shell context"));
    }
    Ok(Some(output))
}

fn separation(
    geometry: &[GeometryProof],
    obstacles: &[ObstacleProof],
) -> Result<f32, CaptureError> {
    let mut minimum = f32::INFINITY;
    for effect in geometry {
        for obstacle in obstacles {
            let gap = rectangle_gap(
                (effect.minimum.into(), effect.maximum.into()),
                (obstacle.minimum.into(), obstacle.maximum.into()),
            );
            if !gap.is_finite() || gap < 12.0 {
                return Err(failure(format!(
                    "effect {} is only {gap}px from {} {} (requires12px)",
                    effect.entity, obstacle.kind, obstacle.entity
                )));
            }
            minimum = minimum.min(gap);
        }
    }
    if !minimum.is_finite() {
        return Err(failure(
            "foreground separation has no actual geometry pairs",
        ));
    }
    Ok(minimum)
}

impl OwnershipState {
    /// Verifies real visible geometry against the normalized gameplay ROI and pixel resolution.
    /// Returns `None` only while required geometry or nonzero effect age is still pending.
    pub(in crate::runtime_capture) fn verify(
        &self,
        world: &mut World,
        view: View,
        region: [f32; 4],
        resolution: [u32; 2],
    ) -> Result<Option<OwnershipProof>, CaptureError> {
        let projection = ProjectionView::load(world, region, resolution)?;
        let mut geometry = Vec::new();
        let mut obstacle_proofs = Vec::new();
        let mut minimum_separation_pixels = None;
        if view == View::NeutralTower {
            let Some(proof) = tower_proof(world, &projection)? else {
                return Ok(None);
            };
            geometry.push(proof);
        }
        if let View::Effect(kind) = view {
            if !self.sample_ready(world, kind)? {
                return Ok(None);
            }
            let Some(proofs) = effect_geometry(world, &projection, kind)? else {
                return Ok(None);
            };
            let Some(actual_obstacles) = obstacles(world, &projection)? else {
                return Ok(None);
            };
            minimum_separation_pixels = Some(separation(&proofs, &actual_obstacles)?);
            obstacle_proofs = actual_obstacles;
            geometry = proofs;
        }
        let bubbles = bubble_proofs(world, &projection)?;
        if view == View::Swarm {
            let palette = world.resource::<PlayerPalette>();
            let fixture = crate::shell_order_audit::fixture::participants(world);
            let expected_count = fixture.map_or(200, |ids| ids.len());
            if fixture.is_some_and(|ids| {
                ids.iter().any(|id| {
                    !bubbles
                        .iter()
                        .any(|bubble| bubble.entity == *id && bubble.pixel.is_some())
                })
            }) {
                return Err(failure(
                    "controlled fixture requires both declared real shells inside ROI",
                ));
            }
            if bubbles.len() != expected_count
                || [palette.primary(), palette.secondary()]
                    .iter()
                    .any(|owner| {
                        !bubbles
                            .iter()
                            .any(|bubble| bubble.owner == owner.0 && bubble.pixel.is_some())
                    })
            {
                return Err(failure(
                    "actual swarm count/visible authored roles are missing",
                ));
            }
        }
        let palette = world.resource::<PlayerPalette>();
        let mut labels = Vec::new();
        for player in &world
            .resource::<client::battle_pipeline::session::BattleSession>()
            .metadata
            .players
        {
            let expected = palette
                .resolve(player.owner)
                .map_err(|error| failure(error.to_string()))?
                .ui;
            if player.color != expected {
                return Err(failure("committed label hue differs from palette"));
            }
            labels.push(LabelProof {
                owner: player.owner.0,
                label: player.label.clone(),
                srgb_u8: [expected.r(), expected.g(), expected.b()],
            });
        }
        Ok(Some(OwnershipProof {
            labels,
            bubbles,
            effects: self.effects.values().cloned().collect(),
            geometry,
            obstacles: obstacle_proofs,
            minimum_separation_pixels,
            frozen: super::freeze::Fingerprint::capture(world)?,
            stage_origin: matches!(view, View::Effect(_))
                .then(|| self.stage.map(|stage| stage.origin))
                .flatten(),
            stage_focus: matches!(view, View::Effect(_))
                .then(|| self.stage.map(|stage| stage.focus.to_array()))
                .flatten(),
            preview_effect: if let View::Effect(kind) = view {
                Some(format!("{kind:?}"))
            } else {
                None
            },
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rectangle_distance_handles_overlap_diagonal_and_exact_boundary() {
        let a = (Vec2::ZERO, Vec2::ONE);
        assert!(rectangle_gap(a, a).abs() < f32::EPSILON);
        assert!(
            (rectangle_gap(a, (Vec2::new(13.0, 0.0), Vec2::new(14.0, 1.0))) - 12.0).abs()
                < f32::EPSILON
        );
        assert!(
            (rectangle_gap(a, (Vec2::new(4.0, 5.0), Vec2::new(5.0, 6.0))) - 5.0).abs()
                < f32::EPSILON
        );
    }
}
