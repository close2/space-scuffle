//! Applies the fog of war to the scene of a person playing one side.
//!
//! # Responsibility
//! [`refresh_battle_fog`] copies the viewer's knowledge from [`HudModel`] into
//! [`BattleFog`], and [`apply_battle_fog`] makes the bubbles obey it: an enemy outside `Visible`
//! cells is hidden, and an enemy that was being inspected stops being inspected. Enemies are hidden
//! and not shrouded, and an enemy structure disappears with sight (decision D-10).
//!
//! # How it fits
//! Both run in `PreparePresentation`, after scene sync, so a bubble spawned this frame is hidden
//! on the frame it appears. Hiding is the whole mechanism: Bevy's visibility propagation carries it
//! to the bars and rings, [`pick_unit`](crate::battle_input) skips what is not visible, and smog
//! skips hidden bubbles, so none of those needs to know fog exists.
//!
//! # Invariants
//! - Runs only under the `Player` profile, so `Sandbox` keeps every bubble as it was.
//! - The viewer's own bubbles are always shown. A bubble of anyone else is shown only in sight.
//! - No enemy stays inspected once it is hidden, not even for a frame of the HUD. It is dropped
//!   for good, so that its return to sight does not fill the panel by itself and give it away.
//! - O(N) in the bubbles per frame, with no allocation.

use bevy::prelude::*;

use super::model::HudModel;
use crate::battle_pipeline::fog::BattleFog;
use crate::battle_pipeline::session::BattleSession;
use crate::battle_selection::BattleSelection;
use crate::bubble_shader::BubbleInstance;
use crate::scene_controller::LiveBattleBubble;

/// Brings [`BattleFog`] up to date with the view [`HudModel`] holds.
///
/// Costs one comparison on a frame without a new tick. The fog is only written on a change, so its
/// change detection means something to the billboards that read it.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn refresh_battle_fog(
    session: Res<'_, BattleSession>,
    model: Res<'_, HudModel>,
    mut fog: ResMut<'_, BattleFog>,
) {
    let current = session.world().and_then(|world| {
        let generation = session.commands.generation()?;
        let viewer = model.viewer?;
        Some((world, generation, viewer))
    });
    let identity = current.map(|(_, generation, viewer)| (generation, viewer, model.view.tick));
    if fog.is_current(identity) {
        return;
    }
    match current {
        Some((world, generation, _)) => {
            let nodes = world.map.resources().iter();
            fog.refill(
                generation,
                &model.view,
                nodes.map(|node| node.remaining > 0),
            );
        }
        None => fog.close(),
    }
}

/// Hides what the viewer cannot see, and stops inspecting an enemy that has gone.
///
/// A unit is judged at the position it is drawn at, which is what the player would see.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn apply_battle_fog(
    fog: Res<'_, BattleFog>,
    mut selection: ResMut<'_, BattleSelection>,
    mut bubbles: Query<'_, '_, (&BubbleInstance, &mut Visibility), With<LiveBattleBubble>>,
) {
    for (bubble, mut visibility) in &mut bubbles {
        let [x, y, _] = bubble.0.world_pos;
        let shown = fog.shows(bubble.0.owner, x, y);
        visibility.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if !shown && selection.inspected() == Some(bubble.0.entity_id) {
            selection.stop_inspecting();
        }
    }
}
