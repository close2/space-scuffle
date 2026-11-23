//! The rally of a selected production structure, drawn on the ground (plan
//! `2026-11-23-production-queue` D-10).
//!
//! # Responsibility
//! Owns a pool of rally reticles and rally lines. While a structure of the local player with a
//! rally is selected, one reticle stands at the rally point and one thin ground line runs from the
//! structure's centre to it; every other pooled pair is hidden.
//!
//! # How it fits
//! Reads the published [`PlayerView::rallies`](sim_types::PlayerView::rallies) of the HUD model,
//! which the simulation resolves every tick (a rally to a unit follows that unit), the selection,
//! and the world for the structure's centre and the ground's altitude. It runs in
//! `BattleSet::Present`, after the HUD model was refreshed for the frame, and writes nothing but
//! the pool's transforms and visibility. The reticle uses the selection ring's material with the
//! rally tint, the line an unlit material of the same tint.
//!
//! # Invariants
//! - The pool grows to the most rallies ever shown at once and never shrinks, so a steady state
//!   spawns nothing and allocates nothing; unused pairs are hidden.
//! - A pair is shown only for a selected structure whose rally the view lists; a rally of another
//!   player is never in the view.
//! - Simulation values become `f32` only here, on the way to the screen (charter §3.2).

use bevy::prelude::*;
use sim_types::{EntityId, SimScalar, SimVec2};

use crate::battle_hud::model::HudModel;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_selection::BattleSelection;
use crate::indicators::{IndicatorAssets, SelectionRingMaterial, SelectionRingUniformGpu};

/// Linear RGBA tint of the rally reticle, line and order marker.
pub const RALLY_COLOR: Vec4 = Vec4::new(1.0, 0.82, 0.2, 1.0);

/// Width of the rally reticle in world units.
const RETICLE_SIZE: f32 = 1.6;

/// Width of the rally line in world units.
const LINE_WIDTH: f32 = 0.08;

/// Height above the ground in world units, which keeps both quads out of the terrain surface.
const GROUND_CLEARANCE: f32 = 0.07;

/// Reticle pulse speed, in radians per second.
const RETICLE_PULSE_SPEED: f32 = 4.0;

/// Which half of a pooled pair an entity is.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub enum RallyMarker {
    /// The reticle at the rally point, slot index.
    Reticle(usize),
    /// The line from the structure to the rally point, slot index.
    Line(usize),
}

/// The pool's materials and its size.
#[derive(Resource, Debug)]
pub struct RallyMarkerAssets {
    reticle_material: Handle<SelectionRingMaterial>,
    line_material: Handle<StandardMaterial>,
    mesh: Handle<Mesh>,
    /// Pairs spawned so far: the most rallies ever shown at once.
    pub pool: usize,
}

/// Creates the rally materials; the pool starts empty and grows on first use.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn setup_rally_markers(
    mut commands: Commands<'_, '_>,
    indicators: Res<'_, IndicatorAssets>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
    mut materials: ResMut<'_, Assets<StandardMaterial>>,
) {
    let tint = Color::linear_rgba(RALLY_COLOR.x, RALLY_COLOR.y, RALLY_COLOR.z, RALLY_COLOR.w);
    commands.insert_resource(RallyMarkerAssets {
        reticle_material: ring_materials.add(SelectionRingMaterial {
            uniform: SelectionRingUniformGpu {
                team_color: RALLY_COLOR,
                radius: 1.0,
                is_selected: 1,
                pulse_speed: RETICLE_PULSE_SPEED,
                padding: 0.0,
            },
        }),
        line_material: materials.add(StandardMaterial {
            base_color: tint,
            emissive: LinearRgba::from(tint) * 1.5,
            unlit: true,
            ..default()
        }),
        mesh: indicators.selection_plane_mesh.clone(),
        pool: 0,
    });
}

/// One rally to draw: the structure's centre and the rally point, Bevy world space.
fn shown_rally(model: &HudModel, session: &BattleSession, id: EntityId) -> Option<(Vec3, Vec3)> {
    let world = session.world()?;
    let index = model
        .view
        .rallies
        .binary_search_by_key(&id, |rally| rally.entity)
        .ok()?;
    let [x, y] = model.view.rallies[index].point;
    let structure = world.entities.get(&id)?.position;
    let ground = |point: SimVec2| {
        Vec3::new(
            point.x.to_f32(),
            world.map.terrain_altitude_at(point) + GROUND_CLEARANCE,
            point.y.to_f32(),
        )
    };
    let rally = SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
    Some((ground(structure), ground(rally)))
}

/// Places a reticle at each selected structure's rally point and a line to it, and hides the rest.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn update_rally_markers(
    mut commands: Commands<'_, '_>,
    session: Option<Res<'_, BattleSession>>,
    model: Option<Res<'_, HudModel>>,
    selection: Res<'_, BattleSelection>,
    assets: Option<ResMut<'_, RallyMarkerAssets>>,
    mut markers: Query<'_, '_, (&RallyMarker, &mut Transform, &mut Visibility)>,
    mut shown: Local<'_, Vec<(Vec3, Vec3)>>,
) {
    let Some(mut assets) = assets else {
        return;
    };
    // PERFORMANCE RATIONALE:
    // Why: This runs every frame of a battle.
    // What: Refill a retained list of the rallies to show, then walk the pool once.
    // Trade-off: The list keeps the capacity of the largest selection with rallies.
    shown.clear();
    if let (Some(session), Some(model)) = (session.as_deref(), model.as_deref()) {
        shown.extend(
            selection
                .ids()
                .iter()
                .filter_map(|id| shown_rally(model, session, *id)),
        );
    }
    let wanted = shown.len();
    while assets.pool < wanted {
        let slot = assets.pool;
        commands.spawn((
            RallyMarker::Reticle(slot),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.reticle_material.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        commands.spawn((
            RallyMarker::Line(slot),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.line_material.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        assets.pool += 1;
    }
    for (marker, mut transform, mut visibility) in &mut markers {
        let (RallyMarker::Reticle(slot) | RallyMarker::Line(slot)) = *marker;
        let Some(&(from, to)) = shown.get(slot) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let placed = match *marker {
            RallyMarker::Reticle(_) => {
                Transform::from_translation(to).with_scale(Vec3::splat(RETICLE_SIZE))
            }
            RallyMarker::Line(_) => line_transform(from, to),
        };
        transform.set_if_neq(placed);
        visibility.set_if_neq(Visibility::Inherited);
    }
}

/// The transform that lays the unit plane along the ground from `from` to `to`, `LINE_WIDTH`
/// wide.
#[must_use]
pub fn line_transform(from: Vec3, to: Vec3) -> Transform {
    let span = to - from;
    let length = Vec2::new(span.x, span.z).length();
    let heading = (-span.z).atan2(span.x);
    Transform::from_translation((from + to) * 0.5)
        .with_rotation(Quat::from_rotation_y(heading))
        .with_scale(Vec3::new(length.max(f32::EPSILON), 1.0, LINE_WIDTH))
}

#[cfg(test)]
mod tests {
    use super::line_transform;
    use bevy::prelude::*;

    #[test]
    fn a_line_runs_from_its_start_to_its_end_along_the_ground() {
        for (from, to) in [
            (Vec3::new(1.0, 0.1, 1.0), Vec3::new(5.0, 0.1, 1.0)),
            (Vec3::new(2.0, 0.1, 3.0), Vec3::new(-1.0, 0.1, 7.0)),
        ] {
            let transform = line_transform(from, to);
            // The unit plane's ends are at local x = ±0.5.
            let start = transform.transform_point(Vec3::new(-0.5, 0.0, 0.0));
            let end = transform.transform_point(Vec3::new(0.5, 0.0, 0.0));
            assert!(start.distance(from) < 1e-4, "{start} against {from}");
            assert!(end.distance(to) < 1e-4, "{end} against {to}");
        }
    }
}
