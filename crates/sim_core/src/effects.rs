//! # The effect-tree engine (plan `2026-11-10-spellcaster-mechanics`, Amendment 1)
//!
//! ## Responsibility
//! Owns the live state effect trees leave behind (statuses, persistents, missiles and
//! displacements, [`state`]) and the engine that evaluates the flattened trees of the spell
//! catalog ([`crate::catalog::SpellCatalog`]): the iterative walk evaluator with its target
//! contexts (D-15, D-16), the leaf actions (D-9), the phase 2 sub-steps for missiles,
//! persistents, statuses and displacements (D-4, D-5, D-16), and the unit filters.
//!
//! ## How it fits
//! `SimWorld::effects` holds an [`EffectWorld`] (hashed); `SimWorld::effect_scratch` holds
//! the reused, unhashed buffers. Casts of an `Effect` ability start a walk in phase 1 or in
//! movement; phase 2 runs [`SimWorld::step_effects`] after the hazard zones. The render
//! snapshot reads the instances (D-19).
//!
//! ## Invariants
//! - Every instance is fixed-point and iterated in id or `(EntityId, StatusId)` order (D-16).
//! - The engine never names a spell, unit or status (C5).
//! - A walk allocates nothing once the buffers are warm (D-18).

// PERFORMANCE RATIONALE:
// Why: a storm on a clump applies a status to tens of units each pulse; a Vec per entity would
// allocate the first time each unit is affected, and a boxed node tree would chase pointers on
// every walk.
// What: one world-level status table sorted by (EntityId, StatusId), a flattened node arena
// with child ranges, a fixed-depth explicit evaluation stack, and one reused search buffer
// (sound because searches never nest).
// Trade-off: insertion into the sorted table is O(statuses) and the arena is less readable than
// a recursive enum; in exchange evaluation allocates nothing in steady state and iteration
// order is the hash order.

mod actions;
mod eval;
pub(crate) mod filter;
mod modifiers;
pub mod state;
mod step;

pub use modifiers::StatusModifiers;
pub use state::{
    ActiveMissile, ActivePersistent, ActiveStatus, Displacement, EffectWorld, PersistentAnchor,
};
pub use step::pulse_offset;

use crate::SimWorld;
use sim_types::{EntityId, SimAngle, SimScalar, SimVec2};

/// The high bit of an area id taken by a persistent's area, so it never equals a hazard
/// zone's id in the same frame while both kinds exist (hazard zones go in work package 2b).
pub const PERSISTENT_AREA_ID_BIT: u32 = 1 << 31;

/// How far from its destination a landing unit may be placed when the destination itself is
/// no longer valid (Abduct's `SpawnRange`, plan D-8), in world units.
pub(crate) const LANDING_RING_RANGE: SimScalar = SimScalar::from_raw(2 << 32);

/// Initial capacity of the search buffer: a storm on a 40-unit clump fits.
const SEARCH_CAPACITY: usize = 128;

/// Reused, unhashed buffers of the engine: not state, cleared before each use.
#[derive(Debug)]
pub(crate) struct EffectScratch {
    /// The candidates of the one active search, `(distance², id)` in order.
    pub(crate) search: Vec<(SimScalar, EntityId)>,
    /// Ids of missiles or persistents finished in the current sub-step.
    pub(crate) finished: Vec<u32>,
    /// Units that landed in the current displacement sub-step.
    pub(crate) landed: Vec<EntityId>,
    /// Nodes entered by every walk so far: a diagnostic counter for the work-bound tests,
    /// never read by the simulation.
    pub(crate) visits: u64,
}

impl Default for EffectScratch {
    fn default() -> Self {
        Self {
            search: Vec::with_capacity(SEARCH_CAPACITY),
            finished: Vec::with_capacity(SEARCH_CAPACITY),
            landed: Vec::with_capacity(SEARCH_CAPACITY),
            visits: 0,
        }
    }
}

/// Distances of the placement rings in halves of a world unit (0.5, 1.0, 1.5, 2.0, … ),
/// tried after the centre, eight directions each (plan D-8).
const RING_STEP: SimScalar = SimScalar::HALF;

impl SimWorld {
    /// Nodes entered by every effect walk of this world so far (a test hook for the work
    /// bound of plan D-17; not simulation state and not hashed).
    #[must_use]
    pub fn effect_node_visits(&self) -> u64 {
        self.effect_scratch.visits
    }

    /// Recomputes `id`'s [`StatusModifiers`] from its statuses. The engine calls it after every
    /// insertion or removal of a status of `id`, which keeps the cache on the entity equal to
    /// the fold of the status table.
    pub(crate) fn refresh_status_modifiers(&mut self, id: EntityId) {
        let folded = StatusModifiers::of(&self.effects, self.catalog.spells(), id);
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.status_modifiers = folded;
        }
    }

    /// Starts a walk of the spell catalog's node `root` with `context`, as a cast does.
    ///
    /// For tests and tools that drive the engine directly; casts reach it through
    /// `execute_ability`.
    pub fn run_effect(&mut self, root: sim_types::EffectId, context: sim_types::EffectContext) {
        self.run_effect_walk(root, context);
    }

    /// The first point valid for `id`'s movement layer among `centre`, then the eight
    /// directions (counter-clockwise from +x) at 0.5, 1.0, … up to `range` world units.
    pub(crate) fn first_valid_placement(
        &self,
        id: EntityId,
        centre: SimVec2,
        range: SimScalar,
    ) -> Option<SimVec2> {
        if self
            .validate_displacement_destination_prevalidated(id, centre)
            .is_ok()
        {
            return Some(centre);
        }
        let mut distance = RING_STEP;
        while distance <= range {
            for step in 0..8 {
                let angle = SimAngle::from_degrees(45 * step);
                let point = centre + SimVec2::new(angle.cos(), angle.sin()) * distance;
                if self
                    .validate_displacement_destination_prevalidated(id, point)
                    .is_ok()
                {
                    return Some(point);
                }
            }
            distance = distance.saturating_add(RING_STEP);
        }
        None
    }
}
