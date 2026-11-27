//! Orders given on the minimap (plan `2026-12-01-minimap` D-7).
//!
//! # Responsibility
//! Turns this frame's [`MinimapIntent`] presses into orders through the **same functions** a
//! world press uses: a right press runs [`issue_pointer_order`](super::issue_pointer_order), the
//! world's right click; a left press with a command mode armed runs
//! [`targeting_press`](super::targeting_press), the world's mode click; a right press with a
//! mode armed cancels the mode. Only where the pointer aims differs: the aim is built by
//! [`minimap::pick::aim_at`](crate::minimap::pick::aim_at) from the minimap's retained marks
//! instead of along a world ray.
//!
//! # How it fits
//! Registered by the [`MinimapPlugin`](crate::minimap::MinimapPlugin), so only under the player's
//! profile. Runs in [`BattleSet::CollectIntent`](crate::battle_pipeline::BattleSet::CollectIntent)
//! after the HUD wrote the intent and before the world's pointer system. The world never sees these
//! presses: the pointer is over the HUD, so [`InputOwnership`](crate::input_policy::InputOwnership)
//! denies it the pointer.
//!
//! # Invariants
//! - The world's rules hold: orders only while the battle's controls are open and orders are
//!   open (the match not over), only to the local player's selection, and only at what the
//!   viewer may pick.
//! - Runs its O(marks) pick only on a frame with a press; other frames return at once.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::command_mode::{orders_open, CommandMode};
use super::order_marker::OrderMarkers;
use super::{issue_pointer_order, targeting_press, BattleView, PointerAim, PointerFrame};
use crate::battle_pipeline::session::BattleSession;
use crate::battle_selection::BattleSelection;
use crate::minimap::pick::{aim_at, AimSources};
use crate::minimap::{MinimapFrame, MinimapIntent};
use crate::runtime_battle::MapPresentation;
use crate::terrain::TerrainState;

/// What a minimap press is resolved against, besides the battle view.
#[derive(SystemParam)]
pub struct MinimapAimInputs<'w> {
    intent: Option<Res<'w, MinimapIntent>>,
    frame: Option<Res<'w, MinimapFrame>>,
    objects: Option<Res<'w, MapPresentation>>,
    terrain: Option<Res<'w, TerrainState>>,
}

impl MinimapAimInputs<'_> {
    /// The aim of a press at map point `point`, or `None` before the minimap has a layout.
    fn aim(
        &self,
        view: &BattleView<'_, '_>,
        session: &BattleSession,
        point: Vec2,
    ) -> Option<PointerAim> {
        let frame = self.frame.as_deref()?;
        let terrain = self.terrain.as_deref()?;
        let world = session.world()?;
        let owner = view.owner()?;
        let sources = AimSources {
            layout: frame.layout()?,
            marks: frame.marks(),
            subjects: frame.subjects(),
            bubbles: &session.frames.presented().bubbles,
            objects: self
                .objects
                .as_deref()
                .map_or(&[], |objects| objects.objects.as_slice()),
            map: &terrain.map,
        };
        Some(aim_at(&sources, point, |id, unit_owner| {
            BattleView::can_pick(world, owner, id, unit_owner)
        }))
    }
}

/// Issues the orders of this frame's minimap presses.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn collect_minimap_orders(
    inputs: MinimapAimInputs<'_>,
    view: BattleView<'_, '_>,
    selection: Res<'_, BattleSelection>,
    mut session: ResMut<'_, BattleSession>,
    mut markers: OrderMarkers<'_, '_>,
    mut mode: ResMut<'_, CommandMode>,
) {
    let Some(intent) = inputs.intent.as_deref().copied() else {
        return;
    };
    let (Some(point), Some(owner)) = (intent.aim_left.or(intent.aim_right), view.owner()) else {
        return;
    };
    if !view.is_open(&session) {
        return;
    }
    if intent.aim_right.is_some() && *mode != CommandMode::Normal {
        *mode = CommandMode::Normal;
        return;
    }
    let Some(aim) = inputs.aim(&view, &session, point) else {
        return;
    };
    let frame = PointerFrame {
        owner,
        shift: intent.shift,
        cursor: None,
    };
    if intent.aim_right.is_some() {
        if !selection.ids().is_empty() && session.world().is_none_or(orders_open) {
            issue_pointer_order(aim, frame, &selection, &mut session, &mut markers);
        }
    } else if *mode != CommandMode::Normal {
        targeting_press(
            aim,
            frame,
            &mut mode,
            &selection,
            &mut session,
            &mut markers,
        );
    }
}
