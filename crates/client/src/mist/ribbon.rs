//! Immutable slot topology and the retained CPU geometry oracle for movement trails.
//!
//! Production owns one fixed integer-topology mesh per slot; the vertex shader expands
//! it from persistent samples. The CPU implementation remains an independent reference
//! for testing positions, attributes, and conservative bounds against actual GPU output.

use super::{emission::EmitterHistory, MistError, MistStyle};
use bevy::prelude::*;
use bevy::render::mesh::{Indices, VertexAttributeValues};
use bevy::render::primitives::Aabb;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;

const STRANDS: usize = 3;
const WIDTH_BASE: f32 = 0.65;
const WIDTH_GROWTH: f32 = 0.45;
const STRAND_OFFSET: f32 = 0.30;
const CURL_MAX: f32 = 0.18;
const VERTICAL_DRIFT: f32 = 0.12;

/// Conservative world-distance bound around a retained sample, including every strand.
pub(super) fn maximum_sample_excursion(radius: f32, style: &MistStyle) -> f32 {
    radius
        * (style.width_radii * (WIDTH_BASE + WIDTH_GROWTH)
            + STRAND_OFFSET
            + CURL_MAX
            + VERTICAL_DRIFT)
}

/// Allocates all mesh attributes and indices once for the validated sample budget.
///
/// # Errors
/// Returns [`MistError`] when the sample count is outside the validated 4..128 budget.
pub fn allocate_mesh(samples: usize) -> Result<Mesh, MistError> {
    if !(4..=128).contains(&samples) {
        return Err(MistError::GeometryContract);
    }
    let vertices = samples * 2 * STRANDS;
    let mut indices = Vec::with_capacity((samples - 1) * 6 * STRANDS);
    for strand in 0..STRANDS {
        for segment in 0..samples - 1 {
            let start = ((strand * samples + segment) * 2) as u32;
            indices.extend_from_slice(&[
                start,
                start + 1,
                start + 2,
                start + 2,
                start + 1,
                start + 3,
            ]);
        }
    }
    Ok(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0; 3]; vertices])
            .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; vertices])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0; 2]; vertices])
            .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0; 2]; vertices])
            .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0; 4]; vertices])
            .with_inserted_indices(Indices::U32(indices)),
    )
}

/// Integer topology identity: chronological knot, edge, strand and emitter slot.
/// The reserved attribute ID distinguishes this contract from Bevy's standard attributes.
pub const ATTRIBUTE_MIST_VERTEX: bevy::render::mesh::MeshVertexAttribute =
    bevy::render::mesh::MeshVertexAttribute::new(
        "MistTopology",
        0x5353_4d49_5354,
        bevy::render::render_resource::VertexFormat::Uint32x4,
    );

/// Creates immutable topology with explicit slot identity and no dynamic vertex data.
///
/// # Errors
/// Returns [`MistError::GeometryContract`] for an invalid configured sample count.
pub fn allocate_slot_mesh(samples: usize, slot: u32) -> Result<Mesh, MistError> {
    if !(4..=128).contains(&samples) {
        return Err(MistError::GeometryContract);
    }
    let mut vertices = Vec::with_capacity(samples * 2 * STRANDS);
    let mut indices = Vec::with_capacity((samples - 1) * 6 * STRANDS);
    for strand in 0..STRANDS {
        for knot in 0..samples {
            for edge in 0..2 {
                vertices.push([knot as u32, edge, strand as u32, slot]);
            }
        }
        for segment in 0..samples - 1 {
            let start = ((strand * samples + segment) * 2) as u32;
            indices.extend_from_slice(&[
                start,
                start + 1,
                start + 2,
                start + 2,
                start + 1,
                start + 3,
            ]);
        }
    }
    Ok(
        Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::all())
            .with_inserted_attribute(ATTRIBUTE_MIST_VERTEX, vertices)
            .with_inserted_indices(Indices::U32(indices)),
    )
}

/// Geometry inputs in world space, with radius in world units.
pub struct RibbonPose {
    /// Render object's origin; mesh vertices are relative to this position.
    pub origin: Vec3,
    /// Offset applied to historical positions (Visual Lab conveyor only).
    pub history_offset: Vec3,
    /// Current camera position in world space.
    pub camera: Vec3,
    /// Rendered bubble radius.
    pub radius: f32,
    /// Whether overclock should accent the trail.
    pub boosted: bool,
}

/// Overwrites existing attributes and returns a conservative mesh-local bounding box.
///
/// # Errors
/// Returns [`MistError`] if the mesh's preallocated attribute contract has been changed.
pub fn update_mesh(
    mesh: &mut Mesh,
    history: &EmitterHistory,
    pose: &RibbonPose,
    style: &MistStyle,
) -> Result<Aabb, MistError> {
    let count = style.samples_per_emitter;
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    // PERFORMANCE RATIONALE:
    // Why: Mesh replacement or temporary attribute vectors churn CPU/GPU storage every frame.
    // What: Rewrite each existing attribute in place using the same pure vertex calculation.
    // Trade-off: Five short geometry passes avoid temporary buffers and aliasing unsafe code.
    for attribute in [
        Mesh::ATTRIBUTE_POSITION,
        Mesh::ATTRIBUTE_NORMAL,
        Mesh::ATTRIBUTE_UV_0,
        Mesh::ATTRIBUTE_UV_1,
        Mesh::ATTRIBUTE_COLOR,
    ] {
        let values = mesh
            .attribute_mut(attribute)
            .ok_or(MistError::GeometryContract)?;
        if values.len() != count * 2 * STRANDS {
            return Err(MistError::GeometryContract);
        }
        for strand in 0..STRANDS {
            for knot in 0..count {
                let sample_index = knot.min(history.len().saturating_sub(1));
                let sample = history.sample(sample_index).unwrap_or_default();
                let before = history
                    .sample(sample_index.saturating_sub(1))
                    .unwrap_or(sample);
                let after = history
                    .sample((sample_index + 1).min(history.len().saturating_sub(1)))
                    .unwrap_or(sample);
                let tangent = (after.position - before.position)
                    .try_normalize()
                    .unwrap_or(Vec3::X);
                let center = sample.position + pose.history_offset;
                let view = (pose.camera - center).normalize_or_zero();
                let side = tangent.cross(view).try_normalize().unwrap_or(Vec3::Z);
                let age = ((history.clock() - sample.birth) as f32 / style.lifetime_seconds)
                    .clamp(0.0, 1.0);
                let tail = (1.0 - age).sqrt();
                let width_factor = if strand == 0 { 1.0 } else { 0.20 };
                let offset = if strand == 0 {
                    0.0
                } else {
                    (strand as f32 * 2.0 - 3.0) * STRAND_OFFSET
                };
                let phase = sample.distance / pose.radius;
                let curl = (phase * 1.9 + strand as f32 * 2.1).sin() * age * CURL_MAX;
                let width = pose.radius
                    * style.width_radii
                    * width_factor
                    * (WIDTH_BASE + age * WIDTH_GROWTH)
                    * tail;
                for edge in 0..2 {
                    let across = edge as f32 * 2.0 - 1.0;
                    let position = center - pose.origin
                        + side * (across * width + pose.radius * (offset + curl) * tail)
                        + Vec3::Y * (age * pose.radius * VERTICAL_DRIFT);
                    let index = (strand * count + knot) * 2 + edge;
                    let coverage = if knot < history.len() && history.len() > 1 {
                        if strand == 0 {
                            1.0
                        } else {
                            0.95
                        }
                    } else {
                        0.0
                    };
                    match values {
                        VertexAttributeValues::Float32x3(items)
                            if attribute.id == Mesh::ATTRIBUTE_POSITION.id =>
                        {
                            items[index] = position.to_array();
                            min = min.min(position);
                            max = max.max(position);
                        }
                        VertexAttributeValues::Float32x3(items) => items[index] = view.to_array(),
                        VertexAttributeValues::Float32x2(items)
                            if attribute.id == Mesh::ATTRIBUTE_UV_0.id =>
                        {
                            items[index] = [phase, across];
                        }
                        VertexAttributeValues::Float32x2(items) => {
                            items[index] = [age, f32::from(pose.boosted)];
                        }
                        VertexAttributeValues::Float32x4(items) => {
                            items[index] = [strand as f32, tangent.x, tangent.z, coverage];
                        }
                        _ => return Err(MistError::GeometryContract),
                    }
                }
            }
        }
    }
    Ok(Aabb::from_min_max(min, max))
}
