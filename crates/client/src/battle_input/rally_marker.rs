//! The rallies of selected production structures, drawn on the ground (plans
//! `2026-11-23-production-queue` D-10 and `2026-11-25-rallies` D-8).
//!
//! # Responsibility
//! Owns a pool of rally reticles and rally lines. For each selected structure of the local
//! player, every published waypoint of every rally slot has a reticle, and a thin ground line
//! runs from the structure's centre to the slot's first shown waypoint and from each shown
//! waypoint to the next. A slot's marks take its tint: slot 0 the rally colour, later slots the
//! second colour (5.0.14 `CAbilRally` `OrderArray` 0 and 1, plan S1). Every other pooled pair is
//! hidden.
//!
//! # How it fits
//! Reads the published [`PlayerView::rallies`](sim_types::PlayerView::rallies) of the HUD model,
//! which the simulation resolves every tick (a waypoint on a unit follows that unit, and an
//! enemy's is published only while the viewer sees it), the selection, and the world for the
//! structure's centre and the ground's altitude. It runs in `BattleSet::Present`, after the HUD
//! model was refreshed for the frame, and writes nothing but the pool's transforms, visibility
//! and material handles. The reticle uses the selection ring's material with the slot's tint,
//! the line an unlit material of the same tint.
//!
//! # Invariants
//! - The pool grows to the most waypoints ever shown at once and never shrinks, so a steady
//!   state spawns nothing and allocates nothing; unused pairs are hidden.
//! - A pair is shown only for a waypoint of a selected structure that the view lists; a rally
//!   of another player is never in the view.
//! - Simulation values become `f32` only here, on the way to the screen (charter §3.2).

use bevy::prelude::*;
use sim_types::{EntityId, SimScalar, SimVec2};

use crate::battle_hud::model::HudModel;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_selection::BattleSelection;
use crate::indicators::{IndicatorAssets, SelectionRingMaterial, SelectionRingUniformGpu};

/// Linear RGBA tint of the rally reticle, line and order marker of rally slot 0.
pub const RALLY_COLOR: Vec4 = Vec4::new(1.0, 0.82, 0.2, 1.0);

/// sRGB tint of the marks of rally slots after the first (the worker rally of a structure with
/// one): 5.0.14's `CAbilRally` `OrderArray` 1 colour, `255,245,140,70` (core AbilData:671-701,
/// plan `2026-11-25-rallies` S1).
pub const SECOND_RALLY_SRGB: [u8; 3] = [245, 140, 70];

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
    /// The reticle at a waypoint, pool index.
    Reticle(usize),
    /// The line into a waypoint, from the structure or the waypoint before it, pool index.
    Line(usize),
}

/// The pool's materials, one pair per tint, and its size.
#[derive(Resource, Debug)]
pub struct RallyMarkerAssets {
    /// Reticle materials: slot 0's, then the later slots'.
    reticle_materials: [Handle<SelectionRingMaterial>; 2],
    /// Line materials: slot 0's, then the later slots'.
    line_materials: [Handle<StandardMaterial>; 2],
    mesh: Handle<Mesh>,
    /// Pairs spawned so far: the most waypoints ever shown at once.
    pub pool: usize,
}

/// One waypoint to draw, Bevy world space: where its line starts, the waypoint, and its tint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShownWaypoint {
    /// The structure's centre for a slot's first shown waypoint, else the waypoint before it.
    pub from: Vec3,
    /// The waypoint.
    pub to: Vec3,
    /// Index into the tinted materials: 0 for slot 0, 1 for every later slot.
    pub tint: usize,
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
    let [r, g, b] = SECOND_RALLY_SRGB;
    let second = Color::srgb_u8(r, g, b).to_linear();
    let tints = [
        RALLY_COLOR,
        Vec4::new(second.red, second.green, second.blue, 1.0),
    ];
    let mut reticle = |tint: Vec4| {
        ring_materials.add(SelectionRingMaterial {
            uniform: SelectionRingUniformGpu {
                team_color: tint,
                radius: 1.0,
                is_selected: 1,
                pulse_speed: RETICLE_PULSE_SPEED,
                padding: 0.0,
            },
        })
    };
    let reticle_materials = [reticle(tints[0]), reticle(tints[1])];
    let mut line = |tint: Vec4| {
        let color = Color::linear_rgba(tint.x, tint.y, tint.z, tint.w);
        materials.add(StandardMaterial {
            base_color: color,
            emissive: LinearRgba::from(color) * 1.5,
            unlit: true,
            ..default()
        })
    };
    let line_materials = [line(tints[0]), line(tints[1])];
    commands.insert_resource(RallyMarkerAssets {
        reticle_materials,
        line_materials,
        mesh: indicators.selection_plane_mesh.clone(),
        pool: 0,
    });
}

/// The parts of a pooled marker the update writes: its kind, placement, visibility, and the
/// material handle of a reticle or of a line, whichever it carries.
type PooledMarker<'a> = (
    &'a RallyMarker,
    &'a mut Transform,
    &'a mut Visibility,
    Option<&'a mut MeshMaterial3d<SelectionRingMaterial>>,
    Option<&'a mut MeshMaterial3d<StandardMaterial>>,
);

/// Appends to `shown` every published waypoint of the selected structure `id`, chained per slot
/// (the view lists them sorted by entity, slot and index).
fn push_waypoints(
    model: &HudModel,
    session: &BattleSession,
    id: EntityId,
    shown: &mut Vec<ShownWaypoint>,
) {
    let Some(world) = session.world() else {
        return;
    };
    let Some(structure) = world.entities.get(&id).map(|entity| entity.position) else {
        return;
    };
    let ground = |point: SimVec2| {
        Vec3::new(
            point.x.to_f32(),
            world.map.terrain_altitude_at(point) + GROUND_CLEARANCE,
            point.y.to_f32(),
        )
    };
    let rallies = &model.view.rallies;
    let first = rallies.partition_point(|rally| rally.entity < id);
    let centre = ground(structure);
    let mut previous: Option<(u8, Vec3)> = None;
    for rally in rallies[first..]
        .iter()
        .take_while(|rally| rally.entity == id)
    {
        let [x, y] = rally.point;
        let to = ground(SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y)));
        let from = match previous {
            Some((slot, from)) if slot == rally.slot => from,
            _ => centre,
        };
        shown.push(ShownWaypoint {
            from,
            to,
            tint: usize::from(rally.slot > 0),
        });
        previous = Some((rally.slot, to));
    }
}

/// Places a reticle at each published waypoint of each selected structure and the chained lines
/// to them, tinted by slot, and hides the rest.
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
    mut markers: Query<'_, '_, PooledMarker<'_>>,
    mut shown: Local<'_, Vec<ShownWaypoint>>,
) {
    let Some(mut assets) = assets else {
        return;
    };
    // PERFORMANCE RATIONALE:
    // Why: This runs every frame of a battle.
    // What: Refill a retained list of the waypoints to show, then walk the pool once.
    // Trade-off: The list keeps the capacity of the most waypoints ever shown.
    shown.clear();
    if let (Some(session), Some(model)) = (session.as_deref(), model.as_deref()) {
        for &id in selection.ids() {
            push_waypoints(model, session, id, &mut shown);
        }
    }
    let wanted = shown.len();
    while assets.pool < wanted {
        let slot = assets.pool;
        commands.spawn((
            RallyMarker::Reticle(slot),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.reticle_materials[0].clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        commands.spawn((
            RallyMarker::Line(slot),
            Mesh3d(assets.mesh.clone()),
            MeshMaterial3d(assets.line_materials[0].clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
        assets.pool += 1;
    }
    for (marker, mut transform, mut visibility, reticle, line) in &mut markers {
        let (RallyMarker::Reticle(slot) | RallyMarker::Line(slot)) = *marker;
        let Some(&waypoint) = shown.get(slot) else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        let placed = match *marker {
            RallyMarker::Reticle(_) => {
                Transform::from_translation(waypoint.to).with_scale(Vec3::splat(RETICLE_SIZE))
            }
            RallyMarker::Line(_) => line_transform(waypoint.from, waypoint.to),
        };
        if let Some(mut material) = reticle {
            let wanted = &assets.reticle_materials[waypoint.tint];
            if material.0 != *wanted {
                material.0 = wanted.clone();
            }
        }
        if let Some(mut material) = line {
            let wanted = &assets.line_materials[waypoint.tint];
            if material.0 != *wanted {
                material.0 = wanted.clone();
            }
        }
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
