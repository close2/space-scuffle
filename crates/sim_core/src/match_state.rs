//! Win and loss: who is still in the match, and when it ends.
//!
//! # The rule
//! A player is defeated on the tick they have no structures left. A foundation is a structure,
//! so a player rebuilding is not out. The last undefeated player wins. The simulation has no
//! teams, so every player is their own side; if the last players fall on the same tick the
//! match is a draw.
//!
//! # Who counts
//! A player takes part once they have owned a structure, so a world of bare armies, a test
//! rig or the Visual Lab, never ends by this rule. A match needs two participants to end,
//! and neutral entities take no part.
//!
//! # After the end
//! The world keeps ticking, so projectiles land and the end can be shown, but it refuses every
//! command with [`GameCommandError::MatchOver`](crate::economy::GameCommandError::MatchOver).
//! A defeated player in a larger match likewise loses all command while the others play on.
//! Their remaining units stay and defend themselves.
//!
//! # Invariants
//! - The evaluation reads the map's registered structures after reaping, so every way of
//!   losing a structure, weapon, spell or cancellation, passes through this one check.
//! - The result is set once and never changes, nor do defeats after it.
//! - All of this state is authoritative and part of the world checksum.

use crate::SimWorld;
use sim_types::{MatchResult, PlayerId, Tick};

/// Authoritative standing of the match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MatchState {
    /// Current result.
    pub result: MatchResult,
    /// Bit per player slot: has owned a structure at some point.
    pub participants: u8,
    /// Bit per player slot: has lost every structure.
    pub defeated: u8,
}

impl MatchState {
    /// Whether this player has been defeated.
    #[must_use]
    pub const fn is_defeated(&self, player: PlayerId) -> bool {
        player.0 < 8 && self.defeated & (1 << player.0) != 0
    }
}

impl SimWorld {
    /// Records which players still stand and settles the match if only one, or none, does.
    ///
    /// Runs after reaping on every tick. Scenario construction also runs it once, so a match
    /// knows its players from tick zero.
    pub(crate) fn update_match_state(&mut self) {
        // PERFORMANCE RATIONALE:
        // Why: This runs every tick, and every entity would otherwise need a catalog lookup.
        // What: Walk only the map's structure registry, which is sorted and small.
        // Trade-off: None for readers; entity lookups stay logarithmic.
        let mut standing = 0_u8;
        for id in self.map.structures().keys() {
            let owner = self.entities.get(id).map(|entity| entity.owner.0);
            if let Some(slot) = owner.filter(|slot| *slot < 8) {
                standing |= 1 << slot;
            }
        }
        let state = &mut self.match_state;
        if state.result.is_over() {
            return;
        }
        state.participants |= standing;
        if state.participants.count_ones() < 2 {
            return;
        }
        state.defeated |= state.participants & !standing;
        let undefeated = state.participants & !state.defeated;
        let tick = Tick(self.current_tick);
        match undefeated.count_ones() {
            0 => state.result = MatchResult::Draw { tick },
            1 => {
                let slot = u8::try_from(undefeated.trailing_zeros()).unwrap_or(u8::MAX);
                state.result = MatchResult::Victory {
                    winner: PlayerId(slot),
                    tick,
                };
            }
            _ => {}
        }
    }
}
