//! Local producer egress geometry and refreshed collision admission.
//!
//! Candidate order is clockwise from the expanded rectangle's minimum corner.
//! Every ground exit connects to the actual producer boundary; only that producer's
//! occupancy is waived during emergence, never terrain or another structure.

use super::ProductionInvariant;
use crate::SimWorld;
use sim_types::{EntityId, ShapeKind, SimScalar, SimVec2, UnitTypeId};

impl SimWorld {
    pub(crate) fn rebuild_production_occupancy(&mut self) -> Result<(), ProductionInvariant> {
        self.production.occupancy.clear();
        self.production.max_radius = SimScalar::ZERO;
        for entity in self
            .entities
            .values()
            .filter(|e| e.is_alive() && !e.is_loaded_in_transport())
        {
            let arch = self.catalog.get(entity.unit_type_id).map_err(|_| {
                ProductionInvariant::MissingArchetype {
                    entity: entity.id,
                    unit_type: entity.unit_type_id,
                }
            })?;
            if arch.collision_radius <= SimScalar::ZERO {
                return Err(ProductionInvariant::InvalidDefinition {
                    unit_type: entity.unit_type_id,
                    field: "collision_radius",
                });
            }
            self.production.max_radius = self.production.max_radius.max(arch.collision_radius);
            self.production.occupancy.insert(entity.id, entity.position);
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one exit search; splitting is plan step 9.2"
    )]
    pub(crate) fn production_exit(
        &mut self,
        producer: EntityId,
        product: UnitTypeId,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        let entity = self
            .entities
            .get(&producer)
            .ok_or(ProductionInvariant::EntityState {
                entity: producer,
                reason: "ready producer disappeared",
            })?;
        let source = self.catalog.get(entity.unit_type_id).map_err(|_| {
            ProductionInvariant::MissingArchetype {
                entity: producer,
                unit_type: entity.unit_type_id,
            }
        })?;
        let target =
            self.catalog
                .get(product)
                .map_err(|_| ProductionInvariant::MissingArchetype {
                    entity: producer,
                    unit_type: product,
                })?;
        let radius = target.collision_radius;
        let airborne = crate::movement::MovementLayer::from_mover(target.mover_type)
            == crate::movement::MovementLayer::Air;
        if radius <= SimScalar::ZERO {
            return Err(ProductionInvariant::InvalidDefinition {
                unit_type: product,
                field: "collision_radius",
            });
        }
        let origin = entity.position;
        let (left, bottom, right, top) =
            if matches!(source.shape, ShapeKind::BuildingDome { .. }) {
                let placed = self.map.structures().get(&producer).ok_or(
                    ProductionInvariant::EntityState {
                        entity: producer,
                        reason: "producer foundation absent",
                    },
                )?;
                (
                    i64::from(placed.top_left.0),
                    i64::from(placed.top_left.1),
                    i64::from(placed.top_left.0) + i64::from(placed.width) - 1,
                    i64::from(placed.top_left.1) + i64::from(placed.height) - 1,
                )
            } else {
                let raw = i128::from(source.collision_radius.raw());
                let cell = |position: SimScalar, offset: i128| -> i64 {
                    i64::try_from((i128::from(position.raw()) + offset) >> 32).unwrap_or(i64::MAX)
                };
                (
                    cell(origin.x, -raw),
                    cell(origin.y, -raw),
                    cell(origin.x, raw),
                    cell(origin.y, raw),
                )
            };
        let margin =
            i64::try_from((i128::from(radius.raw()) + (1_i128 << 31) + (1_i128 << 32) - 1) >> 32)
                .unwrap_or(i64::MAX);
        let bounds = (left - margin, bottom - margin, right + margin, top + margin);
        // A product wider than the map cannot have any legal endpoint. Avoid
        // enumerating an enormous authored-radius perimeter in this case.
        if i128::from(radius.raw()) * 2
            > (i128::from(self.map.width().min(self.map.height())) << 32)
        {
            return Ok(None);
        }
        // PERFORMANCE RATIONALE:
        // Why: Ready jobs retry while exits are blocked, without allocating routes.
        // What: Stream the finite clockwise perimeter and query reusable buckets.
        // Trade-off: Conservative cell-center exits can wait despite a sub-cell gap.
        for side in 0..4 {
            let (start, end) = if side % 2 == 0 {
                (bounds.0, bounds.2)
            } else {
                (bounds.1 + 1, bounds.3 - 1)
            };
            for offset in start..=end {
                let (x, y) = match side {
                    0 => (offset, bounds.1),
                    1 => (bounds.2, offset),
                    2 => (bounds.2 - (offset - bounds.0), bounds.3),
                    _ => (bounds.0, bounds.3 - (offset - (bounds.1 + 1)) - 1),
                };
                if x < 0
                    || y < 0
                    || x >= i64::from(self.map.width())
                    || y >= i64::from(self.map.height())
                {
                    continue;
                }
                let point = SimVec2::new(
                    SimScalar::from_raw((x << 32) + (1_i64 << 31)),
                    SimScalar::from_raw((y << 32) + (1_i64 << 31)),
                );
                let raw_radius = i128::from(radius.raw());
                if i128::from(point.x.raw()) - raw_radius < 0
                    || i128::from(point.y.raw()) - raw_radius < 0
                    || i128::from(point.x.raw()) + raw_radius > (i128::from(self.map.width()) << 32)
                    || i128::from(point.y.raw()) + raw_radius
                        > (i128::from(self.map.height()) << 32)
                {
                    continue;
                }
                if !airborne {
                    let anchor = if matches!(source.shape, ShapeKind::BuildingDome { .. }) {
                        SimVec2::new(
                            SimScalar::from_raw((x.clamp(left, right) << 32) + (1_i64 << 31)),
                            SimScalar::from_raw((y.clamp(bottom, top) << 32) + (1_i64 << 31)),
                        )
                    } else {
                        origin
                    };
                    if !self.egress_connected(anchor, point, radius, Some(producer))
                        || !self.egress_connected(point, point, radius, None)
                    {
                        continue;
                    }
                }
                let reach_raw =
                    i128::from(radius.raw()) + i128::from(self.production.max_radius.raw());
                let ceil = ((reach_raw + (1_i128 << 32) - 1) >> 32)
                    .min(i128::from(self.map.width().max(self.map.height())));
                let query_radius =
                    SimScalar::from_raw(i64::try_from(ceil << 32).unwrap_or(i64::MAX));
                self.production.occupancy.query_radius(
                    point,
                    query_radius,
                    &mut self.production.neighbors,
                );
                let mut occupied = false;
                for id in &self.production.neighbors {
                    let other = &self.entities[id];
                    let arch = self.catalog.get(other.unit_type_id).map_err(|_| {
                        ProductionInvariant::MissingArchetype {
                            entity: *id,
                            unit_type: other.unit_type_id,
                        }
                    })?;
                    if (crate::movement::MovementLayer::from_mover(arch.mover_type)
                        == crate::movement::MovementLayer::Air)
                        != airborne
                    {
                        continue;
                    }
                    let dx = (i128::from(point.x.raw()) - i128::from(other.position.x.raw()))
                        .unsigned_abs();
                    let dy = (i128::from(point.y.raw()) - i128::from(other.position.y.raw()))
                        .unsigned_abs();
                    let sum = (i128::from(radius.raw()) + i128::from(arch.collision_radius.raw()))
                        .unsigned_abs();
                    // Each term fits u128; reject by axes before adding squares so
                    // extreme setup coordinates cannot overflow the squared sum.
                    if dx < sum && dy < sum && dx * dx < sum * sum - dy * dy {
                        occupied = true;
                        break;
                    }
                }
                if !occupied {
                    return Ok(Some(point));
                }
            }
        }
        Ok(None)
    }

    fn egress_connected(
        &self,
        start: SimVec2,
        end: SimVec2,
        radius: SimScalar,
        exempt: Option<EntityId>,
    ) -> bool {
        crate::movement::geometry::ground_footprint_connected(&self.map, start, end, radius, exempt)
    }
}

#[cfg(test)]
mod tests {
    use crate::movement::geometry::segment_overlaps_open_box;

    #[test]
    fn swept_cell_intersection_includes_fractional_origin_and_rejects_tangency() {
        let bounds = (0, 0, 100, 100);
        assert!(segment_overlaps_open_box((25, 25), (125, 125), bounds));
        assert!(segment_overlaps_open_box((125, 125), (25, 25), bounds));
        assert!(segment_overlaps_open_box((50, 50), (50, 50), bounds));
        assert!(!segment_overlaps_open_box((-25, 100), (125, 100), bounds));
        assert!(!segment_overlaps_open_box((-25, -25), (0, 0), bounds));
        assert!(!segment_overlaps_open_box((0, 0), (-25, -25), bounds));
        assert!(segment_overlaps_open_box((-25, 50), (125, 50), bounds));
    }

    #[test]
    fn extreme_slabs_obey_the_actual_radius_and_map_bound_contract() {
        let unit = 1_i128 << 32;
        let map_extent = i128::from(i32::MAX) * unit;
        for radius in [1, map_extent / 4, map_extent / 2 - 1] {
            let start = (radius, radius);
            let end = (map_extent - radius, map_extent - radius);
            assert!(start.0 - radius >= 0 && end.0 + radius <= map_extent);
            assert!(end.0 <= i128::from(i64::MAX));
            let cell = (map_extent / 2 / unit) * unit;
            let bounds = (
                cell - radius,
                cell - radius,
                cell + unit + radius,
                cell + unit + radius,
            );
            assert!(segment_overlaps_open_box(start, end, bounds));
            assert!(segment_overlaps_open_box(end, start, bounds));
        }
    }

    #[test]
    fn fractional_origin_wall_overlap_and_exact_tangency_differ_by_one_raw_unit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut map = sim_map::GameMap::new(
            8,
            8,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        map.place_structure(sim_types::EntityId(1), 2, 2, 1, 1)?;
        let world = crate::SimWorld::new(map, 1);
        let radius = sim_types::SimScalar::from_num(0.375);
        let tangent = sim_types::SimVec2::new(
            sim_types::SimScalar::from_num(3.375),
            sim_types::SimScalar::from_num(2.5),
        );
        let end = sim_types::SimVec2::new(sim_types::SimScalar::from_num(4.5), tangent.y);
        assert!(world.egress_connected(tangent, end, radius, None));
        let overlap = sim_types::SimVec2::new(
            sim_types::SimScalar::from_raw(tangent.x.raw() - 1),
            tangent.y,
        );
        assert!(!world.egress_connected(overlap, end, radius, None));
        assert!(!world.egress_connected(end, overlap, radius, None));
        Ok(())
    }
}
