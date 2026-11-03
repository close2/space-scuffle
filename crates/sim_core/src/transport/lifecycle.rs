//! Atomic death groups for carriers and their direct passengers.
//!
//! Topology and captured supply validate before any group. A group stages every
//! member's release through the production ledger formula, removes its sole possible
//! foundation, then commits membership/economy without another fallible operation.
//! Local preparation failure leaves that group unchanged and permits unrelated roots.

use crate::production::ProductionInvariant;
use crate::{PlayerResources, SimWorld};
use sim_types::{CombatEventVisual, EntityId, PlayerId, PlayerSlot, SimScalar, SimVec2};

struct PreparedDeathGroup {
    economy: PlayerResources,
    owner: PlayerId,
    /// `owner`'s slot, proven while preparing, so the commit writes without a lookup.
    slot: PlayerSlot,
    foundation: bool,
    shatter: Option<(SimVec2, SimScalar)>,
}

impl SimWorld {
    pub(crate) fn reap_transport_groups(&mut self) {
        if let Err(source) = self.validate_transport_links() {
            self.report_transport_invariant(source);
            return;
        }
        if let Err(source) = self.validate_supply_ledger() {
            self.report_production_failure(source);
            return;
        }

        // PERFORMANCE RATIONALE:
        // Why: An inverse scan for each dead carrier makes a crowded reap quadratic.
        // What: Retain one (root, member) buffer, fill with BTreeMap lookups and sort
        //       once so each death group occupies a contiguous ID-ordered slice.
        // Trade-off: O(N log N) preparation and high-water capacity growth; no clone
        //            of entities or duplicated authoritative passenger collection.
        let mut members = std::mem::take(&mut self.transport.death_members);
        members.clear();
        for (&id, actor) in &self.entities {
            let dying_carrier = actor.transport_carrier().filter(|carrier| {
                self.entities
                    .get(carrier)
                    .is_some_and(|source| !source.is_alive())
            });
            if let Some(root) = dying_carrier {
                members.push((root, id));
            } else if !actor.is_alive() {
                members.push((id, id));
            }
        }
        members.sort_unstable();
        let mut start = 0;
        while start < members.len() {
            let root = members[start].0;
            let mut end = start + 1;
            while end < members.len() && members[end].0 == root {
                end += 1;
            }
            match self.prepare_death_group(root, &members[start..end]) {
                Ok(prepared) => {
                    // Only a root can be a foundation: graph validation forbids
                    // foundation passengers. The complete footprint was prepared.
                    // No domain operation after this removal can return an error.
                    if prepared.foundation && self.map.remove_structure(root).is_err() {
                        self.report_production_failure(ProductionInvariant::EntityState {
                            entity: root,
                            reason: "prepared death-group foundation disappeared",
                        });
                        start = end;
                        continue;
                    }
                    for &(_, id) in &members[start..end] {
                        // Exclusive access preserves every prepared member until commit.
                        self.entities.remove(&id);
                    }
                    self.player_resources[prepared.slot] = prepared.economy;
                    if prepared.foundation {
                        self.pathfinder.recompute_clearance(&self.map);
                    }
                    if let Some((position, radius)) = prepared.shatter {
                        self.pending_events.push(CombatEventVisual::BubbleShatter {
                            pos: [
                                position.x.to_f32(),
                                position.y.to_f32(),
                                self.map.terrain_altitude_at(position),
                            ],
                            radius: radius.to_f32(),
                            is_dome: prepared.foundation,
                            owner: prepared.owner,
                        });
                    }
                }
                Err(source) => self.report_production_failure(source),
            }
            start = end;
        }
        members.clear();
        self.transport.death_members = members;
    }

    fn prepare_death_group(
        &self,
        root: EntityId,
        members: &[(EntityId, EntityId)],
    ) -> Result<PreparedDeathGroup, ProductionInvariant> {
        let source = self
            .entities
            .get(&root)
            .ok_or(ProductionInvariant::EntityState {
                entity: root,
                reason: "death-group root is missing",
            })?;
        let owner = source.owner;
        let slot = owner
            .slot()
            .ok_or(ProductionInvariant::InvalidPlayer { player: owner })?;
        let mut economy = self.player_resources[slot];
        let mut foundation = false;
        let mut root_radius = SimScalar::ZERO;
        for &(_, id) in members {
            let actor = self
                .entities
                .get(&id)
                .ok_or(ProductionInvariant::EntityState {
                    entity: id,
                    reason: "death-group member is missing",
                })?;
            if actor.owner != owner {
                return Err(ProductionInvariant::EntityState {
                    entity: id,
                    reason: "death-group owners differ",
                });
            }
            let (released, is_foundation, radius) =
                self.prepare_reap_with_resources(id, economy)?;
            economy = released;
            if id == root {
                foundation = is_foundation;
                root_radius = radius;
            }
        }
        Ok(PreparedDeathGroup {
            economy,
            owner,
            slot,
            foundation,
            shatter: (!source.is_loaded_in_transport()).then_some((source.position, root_radius)),
        })
    }
}
