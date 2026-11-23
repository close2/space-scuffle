//! Rally points of production structures (plan `2026-11-23-production-queue` D-3 to D-7, B-60).
//!
//! # Responsibility
//! A production structure holds at most one [`RallyTarget`] in `Entity::rally`. This module owns
//! the town hall's auto-rally: when an entity whose archetype carries an [`AutoRallySpec`] is
//! created (a scenario's or fixture's `spawn_structure`, or a foundation laid by `Build`), its
//! rally is set to a resource node, as 5.0.14's `CommandStructureAutoRally` does from the town
//! halls' `CreateSet` effects (voidmulti `EffectData` 2507–2537).
//!
//! # How it fits
//! The search reads the map's resource nodes and the archetype's capability, both run-time data;
//! no identity is named. It writes only the created structure's `rally`.
//!
//! # Invariants
//! - The chosen node is standing, has resources left, is of a kind in the spec's set, and its
//!   footprint's cells come within the spec's radius of the structure's centre.
//! - Among those, the node whose centre is farthest from the structure's centre wins
//!   (`TSFarthestDistance`, `MaxCount 1`), the lower node id breaking a tie.
//! - Distances compare squared in raw fixed point, so the choice is exact on every target.
//! - No node qualifies: the rally is left as it was (`None` on a new structure).

use crate::SimWorld;
use sim_types::{AutoRallySpec, EntityId, RallyTarget, ResourceNodeId, SimScalar, SimVec2};

/// The squared distance between two points in raw Q32.32 units squared, exact in `i128`.
fn squared_raw(a: SimVec2, b: SimVec2) -> i128 {
    let dx = i128::from(a.x.raw()) - i128::from(b.x.raw());
    let dy = i128::from(a.y.raw()) - i128::from(b.y.raw());
    dx * dx + dy * dy
}

/// The squared distance in raw units from `point` to the box of cells starting at `cell` with
/// `width` × `height` cells; zero inside the box.
fn squared_raw_to_cells(point: SimVec2, cell: (u32, u32), width: u8, height: u8) -> i128 {
    let axis = |value: SimScalar, start: u32, extent: u8| {
        let low = i128::from(start) << 32;
        let high = (i128::from(start) + i128::from(extent)) << 32;
        let raw = i128::from(value.raw());
        (low - raw).max(raw - high).max(0)
    };
    let dx = axis(point.x, cell.0, width);
    let dy = axis(point.y, cell.1, height);
    dx * dx + dy * dy
}

impl SimWorld {
    /// The node `spec` chooses for a structure centred at `centre` (D-7), if any.
    pub(crate) fn auto_rally_node(
        &self,
        centre: SimVec2,
        spec: AutoRallySpec,
    ) -> Option<ResourceNodeId> {
        let reach = i128::from(spec.radius.raw());
        let reach = reach * reach;
        let mut best: Option<(i128, ResourceNodeId)> = None;
        for (index, node) in (0_u32..).zip(self.map.resources()) {
            let Some(placed) = node.placed else {
                continue;
            };
            if node.remaining == 0 || !spec.kinds.contains(node.kind) {
                continue;
            }
            let cells = placed.footprint;
            if squared_raw_to_cells(centre, cells.top_left, cells.width, cells.height) > reach {
                continue;
            }
            let distance = squared_raw(centre, node.pos);
            // Nodes are visited in id order, so a strictly farther node replaces the best and a
            // tie keeps the lower id.
            if best.is_none_or(|(farthest, _)| distance > farthest) {
                best = Some((distance, ResourceNodeId(index)));
            }
        }
        best.map(|(_, node)| node)
    }

    /// Sets the auto-rally of the newly created structure `id` when its archetype has one (D-7).
    /// A missing entity or archetype, or no qualifying node, leaves the rally untouched; the
    /// caller has just created the entity, so neither is missing in practice.
    pub(crate) fn apply_auto_rally(&mut self, id: EntityId) {
        let Some(entity) = self.entities.get(&id) else {
            return;
        };
        let Some(spec) = self
            .catalog
            .get(entity.unit_type_id)
            .ok()
            .and_then(|arch| arch.capabilities.auto_rally)
        else {
            return;
        };
        let Some(node) = self.auto_rally_node(entity.position, spec) else {
            return;
        };
        if let Some(entity) = self.entities.get_mut(&id) {
            entity.rally = Some(RallyTarget::Node(node));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{squared_raw, squared_raw_to_cells};
    use sim_types::{SimScalar, SimVec2};

    fn point(x: f64, y: f64) -> SimVec2 {
        SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y))
    }

    #[test]
    fn the_box_distance_is_zero_inside_and_the_gap_outside() {
        let one = 1_i128 << 32;
        assert_eq!(squared_raw_to_cells(point(7.0, 14.5), (6, 14), 2, 1), 0);
        // (8.5, 8.5) to the box [6, 8] × [14, 15]: the nearest point is (8, 14), 0.5 and 5.5 away.
        let expected = (one / 2) * (one / 2) + (11 * one / 2) * (11 * one / 2);
        assert_eq!(
            squared_raw_to_cells(point(8.5, 8.5), (6, 14), 2, 1),
            expected
        );
        assert_eq!(
            squared_raw(point(1.0, 2.0), point(4.0, 6.0)),
            25 * one * one
        );
    }
}
