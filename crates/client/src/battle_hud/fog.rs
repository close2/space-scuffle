//! Applies the fog of war to the scene of a person playing one side.
//!
//! # Responsibility
//! [`refresh_battle_fog`] copies the viewer's knowledge from [`HudModel`] into
//! [`BattleFog`], and [`apply_battle_fog`] makes the bubbles obey its [`Presence`]: an enemy
//! outside `Visible` cells is hidden, an undetected burrowed enemy is hidden, an undetected
//! cloaked enemy's bubble is hidden too and loses its selection (all the viewer sees of it is
//! its shimmer, a colourless distortion the effect system holds where it stands,
//! [`crate::effects::stance`]; plan `2026-11-09-effect-visualization` WP8, Q0076), and an
//! enemy that was being inspected
//! stops being inspected once it is no longer [`Presence::Shown`]. Enemies are hidden and not
//! shrouded, and an enemy structure disappears with sight (decision D-10).
//!
//! # How it fits
//! Both run in `PreparePresentation`, after scene sync, so a bubble spawned this frame is hidden
//! on the frame it appears. For a hidden bubble, hiding is the whole mechanism: Bevy's visibility
//! propagation carries it to the bars and rings, and smog skips hidden bubbles. A shimmering
//! unit's bubble is hidden the same way; picking asks
//! [`presence_of`](crate::battle_pipeline::fog::presence_of) itself, and this system also clears
//! the bubble's `selected` flag, which is what the ring and the health bar draw from.
//!
//! # Invariants
//! - Runs only under the `Player` profile, so `Sandbox` keeps every bubble as it was.
//! - The viewer's own bubbles are always shown. A bubble of anyone else is shown only in sight,
//!   and a concealed one only where the viewer detects it (plan
//!   `2026-11-09-effect-visualization` D-5).
//! - No enemy stays inspected once it is hidden or undetected, not even for a frame of the HUD. It
//!   is dropped for good, so that its return to sight does not fill the panel by itself and give
//!   it away.
//! - O(N) in the bubbles per frame, with no allocation.

use bevy::prelude::*;

use super::model::HudModel;
use crate::battle_pipeline::fog::{BattleFog, Presence};
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

/// Hides what the viewer cannot see, and stops inspecting an enemy that has gone or gone
/// undetected.
///
/// A unit is judged at the position it is drawn at, which is what the player would see. Only a
/// [`Presence::Shown`] unit's bubble is drawn: a shimmering unit's look is its shimmer effect.
/// Any unit not shown has its `selected` flag cleared on this frame, before the ring and the
/// health bar read it.
#[expect(
    clippy::needless_pass_by_value,
    reason = "Bevy passes system parameters by value"
)]
pub fn apply_battle_fog(
    fog: Res<'_, BattleFog>,
    mut selection: ResMut<'_, BattleSelection>,
    mut bubbles: Query<'_, '_, (&mut BubbleInstance, &mut Visibility), With<LiveBattleBubble>>,
) {
    for (mut bubble, mut visibility) in &mut bubbles {
        let presence = fog.presence(&bubble.0);
        visibility.set_if_neq(if presence.draws_bubble() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if presence == Presence::Shown {
            continue;
        }
        if selection.inspected() == Some(bubble.0.entity_id) {
            selection.stop_inspecting();
        }
        // `selected` is only read through the change-detecting deref when it must change.
        if bubble.0.selected {
            bubble.0.selected = false;
        }
    }
}
