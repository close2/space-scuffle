//! The hand-off of each resource node's line: the tick from which it may start its next gather.
//!
//! # Responsibility
//! In `StarCraft` II the next worker in a resource's line does not start on the tick the previous
//! one takes its load: the harvest ability's stages hand the resource over a whole engine step
//! or two later (plan `2026-11-23-harvest-handoff` §2.4, **verify**). [`ResourceLines`] records,
//! per node, the tick from which its line may start again: the take's tick plus its kind's
//! `handoff_ticks` ([`ResourceHarvest`](crate::catalog::ResourceHarvest)). It is the one fact of a
//! line that lives across ticks; who gathers and who waits is derived each tick from the units
//! (plan `2026-11-23-harvest-occupancy` D-1).
//!
//! # How it fits
//! The gatherer's step writes a node's release tick at a take that yields cargo, and
//! [`SimWorld::resolve_resource_claims`](crate::SimWorld) reads it, after every step of the tick,
//! counting the node as occupied while `current_tick` is below it (plan
//! `2026-11-23-harvest-handoff` D-1). It decides when a waiter starts, so it is hashed: the
//! world checksum encodes the list after the map's resource nodes (D-3).
//!
//! # Invariants
//! - One entry per node of the map, in the map's node order, sized when the world is built; the
//!   map keeps every node in its list, so identifiers hold and the list never needs to shrink.
//! - Ticks are absolute simulation ticks, not countdowns, so no per-tick pass over the nodes is
//!   needed. A node never taken from reads 0, which no tick is below.
//! - `current_tick` wraps after 2³² ticks (about 5.4 years of play at 25 ticks a second); a
//!   release tick written before a wrap would then read as far in the future. That is
//!   documented, not handled.

use sim_types::ResourceNodeId;

/// Per resource node, the tick from which its line may start its next gather (plan
/// `2026-11-23-harvest-handoff` D-3). Hashed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceLines {
    /// Indexed by [`ResourceNodeId`]: the first simulation tick on which the node's next worker
    /// may be granted it. 0 for a node never taken from.
    release_tick: Vec<u32>,
}

impl ResourceLines {
    /// A list for a map of `nodes` resource nodes, none of them taken from.
    #[must_use]
    pub fn new(nodes: usize) -> Self {
        Self {
            release_tick: vec![0; nodes],
        }
    }

    /// The tick (simulation ticks) from which `node`'s line may start its next gather; 0 for a
    /// node never taken from or beyond the list.
    #[must_use]
    pub fn release_tick(&self, node: ResourceNodeId) -> u32 {
        self.release_tick.get(node.0 as usize).copied().unwrap_or(0)
    }

    /// Whether `node` is in its hand-off at simulation tick `tick`: `tick` is below its release
    /// tick, so no worker may be granted it yet.
    #[must_use]
    pub fn in_handoff(&self, node: ResourceNodeId, tick: u32) -> bool {
        tick < self.release_tick(node)
    }

    /// Records that `node`'s line may start again from simulation tick `tick`. A node beyond the
    /// list grows it, the nodes between reading 0; a world sizes the list for its map, so this
    /// happens only for a node added after the world was built.
    pub fn set_release_tick(&mut self, node: ResourceNodeId, tick: u32) {
        let index = node.0 as usize;
        if index >= self.release_tick.len() {
            self.release_tick.resize(index + 1, 0);
        }
        self.release_tick[index] = tick;
    }

    /// Every node's release tick, in node order.
    #[must_use]
    pub fn release_ticks(&self) -> &[u32] {
        &self.release_tick
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_types::ResourceNodeId;

    /// A new list of `n` nodes reads 0 everywhere: no node has been taken from.
    #[test]
    fn a_new_list_reads_zero_everywhere() {
        let lines = ResourceLines::new(5);
        assert_eq!(lines.release_ticks(), &[0; 5]);
        for node in 0..5 {
            assert_eq!(lines.release_tick(ResourceNodeId(node)), 0);
        }
    }

    /// A write is read back on its own node and on no other.
    #[test]
    fn a_write_is_read_back() {
        let mut lines = ResourceLines::new(3);
        lines.set_release_tick(ResourceNodeId(1), 42);
        assert_eq!(lines.release_tick(ResourceNodeId(1)), 42);
        assert_eq!(lines.release_ticks(), &[0, 42, 0]);
    }

    /// A read past the end gives 0, the release tick of a node never taken from.
    #[test]
    fn a_read_past_the_end_gives_zero() {
        let lines = ResourceLines::new(2);
        assert_eq!(lines.release_tick(ResourceNodeId(2)), 0);
        assert_eq!(lines.release_tick(ResourceNodeId(u32::MAX)), 0);
    }

    /// A write past the end grows the list to hold it, the nodes between reading 0.
    #[test]
    fn a_write_past_the_end_grows_the_list() {
        let mut lines = ResourceLines::new(1);
        lines.set_release_tick(ResourceNodeId(3), 7);
        assert_eq!(lines.release_ticks(), &[0, 0, 0, 7]);
        assert_eq!(lines.release_tick(ResourceNodeId(3)), 7);
    }

    /// A node is in its hand-off strictly before its release tick, and free from it on.
    #[test]
    fn a_node_is_in_its_handoff_before_its_release_tick() {
        let mut lines = ResourceLines::new(1);
        let node = ResourceNodeId(0);
        assert!(!lines.in_handoff(node, 0));
        lines.set_release_tick(node, 12);
        assert!(lines.in_handoff(node, 10));
        assert!(lines.in_handoff(node, 11));
        assert!(!lines.in_handoff(node, 12));
        assert!(!lines.in_handoff(node, 13));
    }
}
