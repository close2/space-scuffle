//! Ground markers that show where an order went.
//!
//! # Responsibility
//! Owns a small pool of entities drawn with the selection reticle's material. Accepting an order
//! shows the next one at the destination; it shrinks and hides itself after
//! [`ORDER_MARKER_SECONDS`].
//!
//! # Invariants
//! - The pool is spawned once, so showing a marker allocates nothing and needs no `Commands`.
//! - A marker confirms that the client accepted an order, not that the unit arrived.
//! - No marker outlives its session. The pool outlives every session, so [`update_order_markers`]
//!   hides all of it on the frame a new generation commits. That system runs in `Present`, after
//!   `CommitSession`, so a marker shown earlier in that frame confirmed an order of the replaced
//!   session, whose queue the commit discarded.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::battle_pipeline::commands::SessionGeneration;
use crate::battle_pipeline::session::BattleSession;
use crate::indicators::{IndicatorAssets, SelectionRingMaterial, SelectionRingUniformGpu};

/// Seconds a marker stays on screen.
pub const ORDER_MARKER_SECONDS: f32 = 0.7;

/// Markers that can show at once. Rapid orders reuse the oldest.
pub const ORDER_MARKER_POOL_SIZE: usize = 6;

/// Width of a marker in world units when it appears.
const MARKER_START_SIZE: f32 = 3.2;

/// Width of a marker in world units just before it hides.
const MARKER_END_SIZE: f32 = 1.2;

/// Height above the ground in world units, which keeps the quad out of the terrain surface.
const MARKER_GROUND_CLEARANCE: f32 = 0.06;

/// Linear RGBA tint of a move marker.
const MOVE_MARKER_COLOR: Vec4 = Vec4::new(0.25, 1.0, 0.45, 1.0);

/// Linear RGBA tint of an attack marker.
const ATTACK_MARKER_COLOR: Vec4 = Vec4::new(1.0, 0.18, 0.12, 1.0);

/// Reticle pulse speed of a marker, in radians per second.
const MARKER_PULSE_SPEED: f32 = 12.0;

/// What a marker confirms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderMarkerKind {
    /// Units were sent to a point on the ground.
    Move,
    /// Units were sent to attack a target.
    Attack,
}

/// One pooled marker. `remaining` is in seconds, and zero means hidden.
#[derive(Component, Debug)]
pub struct OrderMarker {
    /// What this marker last confirmed.
    pub kind: OrderMarkerKind,
    /// Seconds left on screen.
    pub remaining: f32,
    slot: usize,
}

/// Materials of the marker pool, the slot the next order will use, and the session it serves.
#[derive(Resource, Debug)]
pub struct OrderMarkerAssets {
    move_material: Handle<SelectionRingMaterial>,
    attack_material: Handle<SelectionRingMaterial>,
    next_slot: usize,
    /// Session whose orders the visible markers confirm. `None` before the first commit.
    generation: Option<SessionGeneration>,
}

/// Spawns the hidden marker pool.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn setup_order_markers(
    mut commands: Commands<'_, '_>,
    indicators: Res<'_, IndicatorAssets>,
    mut ring_materials: ResMut<'_, Assets<SelectionRingMaterial>>,
) {
    let mut material = |team_color| {
        ring_materials.add(SelectionRingMaterial {
            uniform: SelectionRingUniformGpu {
                team_color,
                radius: 1.0,
                is_selected: 1,
                pulse_speed: MARKER_PULSE_SPEED,
                padding: 0.0,
            },
        })
    };
    let assets = OrderMarkerAssets {
        move_material: material(MOVE_MARKER_COLOR),
        attack_material: material(ATTACK_MARKER_COLOR),
        next_slot: 0,
        generation: None,
    };
    for slot in 0..ORDER_MARKER_POOL_SIZE {
        commands.spawn((
            OrderMarker {
                kind: OrderMarkerKind::Move,
                remaining: 0.0,
                slot,
            },
            Mesh3d(indicators.selection_plane_mesh.clone()),
            MeshMaterial3d(assets.move_material.clone()),
            Transform::default(),
            Visibility::Hidden,
        ));
    }
    commands.insert_resource(assets);
}

/// Access to the marker pool for the systems that issue orders.
#[derive(SystemParam)]
pub struct OrderMarkers<'w, 's> {
    assets: ResMut<'w, OrderMarkerAssets>,
    markers: Query<
        'w,
        's,
        (
            &'static mut OrderMarker,
            &'static mut MeshMaterial3d<SelectionRingMaterial>,
            &'static mut Transform,
            &'static mut Visibility,
        ),
    >,
}

impl OrderMarkers<'_, '_> {
    /// Shows a marker of `kind` on the ground at `position`, in Bevy world space.
    pub fn show(&mut self, kind: OrderMarkerKind, position: Vec3) {
        let slot = self.assets.next_slot;
        self.assets.next_slot = (slot + 1) % ORDER_MARKER_POOL_SIZE;
        let handle = match kind {
            OrderMarkerKind::Move => self.assets.move_material.clone(),
            OrderMarkerKind::Attack => self.assets.attack_material.clone(),
        };
        // The pool is fixed at a handful of entities, so this scan is bounded.
        for (mut marker, mut material, mut transform, mut visibility) in &mut self.markers {
            if marker.slot == slot {
                marker.kind = kind;
                marker.remaining = ORDER_MARKER_SECONDS;
                material.0 = handle;
                *transform =
                    Transform::from_translation(position + Vec3::Y * MARKER_GROUND_CLEARANCE)
                        .with_scale(Vec3::splat(MARKER_START_SIZE));
                *visibility = Visibility::Inherited;
                break;
            }
        }
    }
}

/// Shrinks visible markers and hides the expired ones. When a new session has committed since the
/// last frame, hides every marker instead and restarts the slot cycle.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn update_order_markers(
    time: Res<'_, Time>,
    session: Option<Res<'_, BattleSession>>,
    assets: Option<ResMut<'_, OrderMarkerAssets>>,
    mut markers: Query<'_, '_, (&mut OrderMarker, &mut Transform, &mut Visibility)>,
) {
    let generation = session.and_then(|session| session.commands.generation());
    if let Some(mut assets) = assets.filter(|assets| assets.generation != generation) {
        assets.generation = generation;
        assets.next_slot = 0;
        for (mut marker, _, mut visibility) in &mut markers {
            marker.remaining = 0.0;
            *visibility = Visibility::Hidden;
        }
        return;
    }
    for (mut marker, mut transform, mut visibility) in &mut markers {
        if marker.remaining <= 0.0 {
            continue;
        }
        marker.remaining = (marker.remaining - time.delta_secs()).max(0.0);
        let size = MARKER_END_SIZE.lerp(MARKER_START_SIZE, marker.remaining / ORDER_MARKER_SECONDS);
        transform.scale = Vec3::splat(size);
        if marker.remaining == 0.0 {
            *visibility = Visibility::Hidden;
        }
    }
}
