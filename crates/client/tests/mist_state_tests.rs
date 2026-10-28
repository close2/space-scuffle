//! Frame-rate, geometry, and bounded-storage contracts for the production movement wake.
//!
//! These tests exercise the same histories and mesh writer used by the GPU plugin;
//! visual quality and real shader compilation are verified separately by `mist_preview`.

use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use client::mist::{
    emission::EmitterHistory,
    ribbon::{allocate_mesh, update_mesh, RibbonPose},
    MistError, MistStyle,
};

#[test]
fn actual_shipped_mist_style_loads() -> Result<(), MistError> {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/mist.ron");
    let style = MistStyle::load(&path)?;
    assert!(style.max_emitters >= 400);
    assert!(style.samples_per_emitter >= 4);
    Ok(())
}

fn trajectory(fps: u16) -> Result<EmitterHistory, MistError> {
    let style = MistStyle::default();
    let mut history = EmitterHistory::new(&style)?;
    history.advance(Vec3::ZERO, 1.0, 0.0, &style)?;
    for frame in 1..=fps * 2 {
        let seconds = f32::from(frame) / f32::from(fps);
        history.advance(Vec3::X * (seconds * 2.0), 1.0, 1.0 / f64::from(fps), &style)?;
    }
    Ok(history)
}

#[test]
fn straight_trajectory_is_render_frequency_independent() -> Result<(), MistError> {
    let baseline = trajectory(60)?;
    for fps in [30, 144] {
        let actual = trajectory(fps)?;
        assert_eq!(actual.len(), baseline.len());
        for i in 0..actual.len() {
            let a = actual.sample(i).ok_or(MistError::GeometryContract)?;
            let b = baseline.sample(i).ok_or(MistError::GeometryContract)?;
            assert!(
                a.position.distance(b.position) < 1.0e-4,
                "{fps}: {a:?} != {b:?}"
            );
            assert!((a.birth - b.birth).abs() < 1.0e-4);
        }
    }
    Ok(())
}

#[test]
fn stop_teleport_reset_and_large_delta_keep_bounded_history() -> Result<(), MistError> {
    let style = MistStyle::default();
    let mut history = trajectory(60)?;
    let capacity = history.capacity();
    history.advance(Vec3::X * 100.0, 1.0, 0.1, &style)?;
    assert!(history.is_empty());
    history.advance(Vec3::X * 101.0, 1.0, 1000.0, &style)?;
    assert!(history.len() <= style.samples_per_emitter);
    history.advance(Vec3::X * 101.0, 1.0, 2.0, &style)?;
    assert!(history.is_empty());
    history.reset();
    assert_eq!(history.capacity(), capacity);
    assert_eq!(history.clock(), 0.0);
    Ok(())
}

#[test]
fn mesh_storage_and_bounds_remain_valid_through_turns() -> Result<(), MistError> {
    let style = MistStyle::default();
    let mut history = EmitterHistory::new(&style)?;
    let mut mesh = allocate_mesh(style.samples_per_emitter)?;
    let vertices = mesh.count_vertices();
    let indices = mesh.indices().ok_or(MistError::GeometryContract)?.len();
    let original_position_pointer = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(v)) => v.as_ptr(),
        _ => return Err(MistError::GeometryContract),
    };
    for frame in 0_u16..600 {
        let t = f32::from(frame) / 60.0;
        let position = Vec3::new(t.sin() * 3.0, 1.0, t.cos() * 3.0);
        history.advance(position, 1.0, 1.0 / 60.0, &style)?;
        let bounds = update_mesh(
            &mut mesh,
            &history,
            &RibbonPose {
                origin: position,
                history_offset: Vec3::ZERO,
                camera: Vec3::new(8.0, 8.0, 8.0),
                radius: 1.0,
                boosted: false,
            },
            &style,
        )?;
        assert_eq!(mesh.count_vertices(), vertices);
        assert_eq!(
            mesh.indices().ok_or(MistError::GeometryContract)?.len(),
            indices
        );
        if let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        {
            assert_eq!(positions.as_ptr(), original_position_pointer);
            for position in positions {
                let p = Vec3::from_array(*position);
                assert!(p.is_finite());
                assert!((p - Vec3::from(bounds.center))
                    .abs()
                    .cmple(Vec3::from(bounds.half_extents) + Vec3::splat(1.0e-5))
                    .all());
            }
        }
    }
    Ok(())
}

#[test]
fn malformed_style_and_mesh_budgets_fail_explicitly() {
    assert!(allocate_mesh(0).is_err());
    assert!(allocate_mesh(129).is_err());
    let mut style = MistStyle {
        opacity: f32::NAN,
        ..default()
    };
    assert!(style.validate().is_err());
    style.opacity = 0.4;
    style.samples_per_emitter = 0;
    assert!(style.validate().is_err());
}
