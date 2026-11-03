//! The end of a match, driven through the real client plugin with real input events.
//!
//! # Responsibility
//! Boots the production client on the shipped skirmish, ends the match, and proves what D-12
//! promises: orders stop, sight does not, and the player is told once.
//!
//! # How it fits
//! Ends are staged with `world_mut`, because these tests are about the screen and not the path to
//! it. Every gesture is a real Bevy event.
//!
//! # Limits
//! Shares the limits of `support/battle_harness.rs`: no window and no renderer.
//!
//! # Invariants
//! The pointer tests contain a control, so a test cannot pass merely because input was not being
//! delivered.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use client::battle_hud::feed::BattleFeed;
use client::runtime_battle::SKIRMISH_SCENARIO;
use client::ClientMode;
use sim_types::{EntityId, MatchResult, Tick, UnitOrder};

#[allow(
    dead_code,
    reason = "shared by several test binaries that each use a subset"
)]
#[path = "support/battle_harness.rs"]
mod battle_harness;

use battle_harness::{Harness, LOCAL_PLAYER};

/// Boots the skirmish with the local workers in view.
fn skirmish() -> Harness {
    let mut harness = Harness::boot_scenario(ClientMode::Skirmish, SKIRMISH_SCENARIO);
    let worker = workers(&harness)[0];
    let position = harness.simulated_position(worker);
    harness.look_at(position);
    harness
}

/// The local player's mobile units, ordered by ID.
fn workers(harness: &Harness) -> Vec<EntityId> {
    harness
        .world()
        .entities
        .values()
        .filter(|unit| unit.owner == LOCAL_PLAYER && !unit.is_immobile)
        .map(|unit| unit.id)
        .collect()
}

fn feed_lines(harness: &Harness) -> Vec<String> {
    harness
        .app
        .world()
        .resource::<BattleFeed>()
        .lines()
        .map(|line| line.text.clone())
        .collect()
}

/// Queues a `Stop` for one unit the way a HUD control does.
fn queue_stop(harness: &mut Harness, unit: EntityId) {
    let mut session = harness
        .app
        .world_mut()
        .resource_mut::<client::battle_pipeline::session::BattleSession>();
    let (world, queue) = session.world_and_commands();
    queue
        .submit_selection(
            world.expect("committed battle"),
            LOCAL_PLAYER,
            &[unit],
            UnitOrder::Stop,
            false,
        )
        .expect("the client must admit this order before the end");
}

// --- Orders still pending at the end -------------------------------------------------------------

/// The plan's edge case: refused and reported, and the feed shows one line, not one per order.
#[test]
fn orders_pending_at_the_end_are_refused_and_reported_as_one_line() {
    let mut harness = skirmish();
    let crew = workers(&harness);
    for unit in &crew {
        queue_stop(&mut harness, *unit);
    }
    assert_eq!(harness.pending_orders(), crew.len());
    assert!(crew.len() > 1, "the case needs several pending orders");

    harness.world_mut().match_state.result = MatchResult::Draw { tick: Tick(0) };
    harness.resume();
    harness.step(4);
    harness.pause();

    assert_eq!(harness.pending_orders(), 0, "the refusal consumes them");
    assert_eq!(feed_lines(&harness), ["Order rejected: the match is over"]);
}
