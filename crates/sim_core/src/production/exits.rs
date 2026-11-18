//! Local producer egress geometry and refreshed collision admission.
//!
//! Candidate order is clockwise from the expanded rectangle's minimum corner.
//! Every ground exit connects to the actual producer boundary at the product's terrain
//! radius; only that producer's footprint is waived, on the leg from the footprint to the
//! exit, never terrain or another structure's footprint.
//!
//! Occupancy holds the live units, never registered structures: a structure is not a body
//! and meets the product only through its footprint, which both egress checks test at the
//! terrain radius (plan `2026-11-18-structure-contact` D-2 B4, B-131). Units meet the product
//! at the sum of the collision radii.

use super::ProductionInvariant;
use crate::SimWorld;
use sim_types::{EntityId, ShapeKind, SimScalar, SimVec2, UnitTypeId};

impl SimWorld {
    /// Rebuilds the exit occupancy from the live, unloaded units and their largest collision
    /// radius. Registered structures are left out: they are not bodies (D-2 B4).
    pub(crate) fn rebuild_production_occupancy(&mut self) -> Result<(), ProductionInvariant> {
        self.production.occupancy.clear();
        self.production.max_radius = SimScalar::ZERO;
        for entity in self.entities.values().filter(|e| {
            e.is_alive()
                && !e.is_loaded_in_transport()
                && !self.map.structures().contains_key(&e.id)
        }) {
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

    /// The first free cell centre on the clockwise exit ring around `producer`, in world units,
    /// for a new `product`; `None` while every candidate is blocked or none can exist.
    pub(crate) fn production_exit(
        &mut self,
        producer: EntityId,
        product: UnitTypeId,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        let Some(ring) = self.exit_ring(producer, product)? else {
            return Ok(None);
        };
        // PERFORMANCE RATIONALE:
        // Why: Ready jobs retry while exits are blocked, without allocating routes.
        // What: Stream the finite clockwise perimeter and query reusable buckets.
        // Trade-off: Conservative cell-center exits can wait despite a sub-cell gap.
        for (x, y) in exit_ring_candidates(ring.bounds) {
            if let Some(point) = self.free_exit_point(&ring, x, y)? {
                return Ok(Some(point));
            }
        }
        Ok(None)
    }

    /// Measures the ring of candidate exit cells around `producer` for a new `product`.
    ///
    /// Returns `None` when the product is wider than the map, so no endpoint can be legal.
    fn exit_ring(
        &self,
        producer: EntityId,
        product: UnitTypeId,
    ) -> Result<Option<ExitRing>, ProductionInvariant> {
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
        let terrain_radius = target.terrain_radius;
        let airborne = crate::movement::MovementLayer::from_mover(target.mover_type)
            == crate::movement::MovementLayer::Air;
        if radius <= SimScalar::ZERO {
            return Err(ProductionInvariant::InvalidDefinition {
                unit_type: product,
                field: "collision_radius",
            });
        }
        let origin = entity.position;
        let dome = matches!(source.shape, ShapeKind::BuildingDome { .. });
        let (left, bottom, right, top) =
            if dome {
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
        // The ring keeps the product's terrain disc off the footprint, which a structure meets
        // at that radius (plan `2026-11-18-inner-radius` D-8).
        let margin = i64::try_from(
            (i128::from(terrain_radius.raw()) + (1_i128 << 31) + (1_i128 << 32) - 1) >> 32,
        )
        .unwrap_or(i64::MAX);
        let bounds = (left - margin, bottom - margin, right + margin, top + margin);
        // A product wider than the map cannot have any legal endpoint. Avoid
        // enumerating an enormous authored-radius perimeter in this case.
        if i128::from(terrain_radius.raw()) * 2
            > (i128::from(self.map.width().min(self.map.height())) << 32)
        {
            return Ok(None);
        }
        Ok(Some(ExitRing {
            producer,
            origin,
            footprint: dome.then_some((left, bottom, right, top)),
            bounds,
            radius,
            terrain_radius,
            airborne,
        }))
    }

    /// The centre of exit cell `(x, y)` when the product may emerge there, otherwise `None`.
    ///
    /// A free cell lies on the map with the product's terrain disc, connects to the producer's
    /// boundary on the ground at that disc, and overlaps no unit of the product's own layer at
    /// the product's collision radius (structures are not in the occupancy; their footprints
    /// are the egress checks').
    fn free_exit_point(
        &mut self,
        ring: &ExitRing,
        x: i64,
        y: i64,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        let radius = ring.radius;
        if x < 0 || y < 0 || x >= i64::from(self.map.width()) || y >= i64::from(self.map.height()) {
            return Ok(None);
        }
        let point = SimVec2::new(
            SimScalar::from_raw((x << 32) + (1_i64 << 31)),
            SimScalar::from_raw((y << 32) + (1_i64 << 31)),
        );
        let raw_radius = i128::from(ring.terrain_radius.raw());
        if i128::from(point.x.raw()) - raw_radius < 0
            || i128::from(point.y.raw()) - raw_radius < 0
            || i128::from(point.x.raw()) + raw_radius > (i128::from(self.map.width()) << 32)
            || i128::from(point.y.raw()) + raw_radius > (i128::from(self.map.height()) << 32)
        {
            return Ok(None);
        }
        if !ring.airborne {
            let anchor = if let Some((left, bottom, right, top)) = ring.footprint {
                SimVec2::new(
                    SimScalar::from_raw((x.clamp(left, right) << 32) + (1_i64 << 31)),
                    SimScalar::from_raw((y.clamp(bottom, top) << 32) + (1_i64 << 31)),
                )
            } else {
                ring.origin
            };
            let terrain = ring.terrain_radius;
            if !self.egress_connected(anchor, point, terrain, Some(ring.producer))
                || !self.egress_connected(point, point, terrain, None)
            {
                return Ok(None);
            }
        }
        let reach_raw = i128::from(radius.raw()) + i128::from(self.production.max_radius.raw());
        let ceil = ((reach_raw + (1_i128 << 32) - 1) >> 32)
            .min(i128::from(self.map.width().max(self.map.height())));
        let query_radius = SimScalar::from_raw(i64::try_from(ceil << 32).unwrap_or(i64::MAX));
        self.production
            .occupancy
            .query_radius(point, query_radius, &mut self.production.neighbors);
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
                != ring.airborne
            {
                continue;
            }
            let dx =
                (i128::from(point.x.raw()) - i128::from(other.position.x.raw())).unsigned_abs();
            let dy =
                (i128::from(point.y.raw()) - i128::from(other.position.y.raw())).unsigned_abs();
            let sum =
                (i128::from(radius.raw()) + i128::from(arch.collision_radius.raw())).unsigned_abs();
            // Each term fits u128; reject by axes before adding squares so
            // extreme setup coordinates cannot overflow the squared sum.
            if dx < sum && dy < sum && dx * dx < sum * sum - dy * dy {
                return Ok(None);
            }
        }
        Ok(Some(point))
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

/// The ring of candidate exit cells around one producer, measured once per exit search.
struct ExitRing {
    producer: EntityId,
    /// The producer's position in world units, which a non-structure producer's exits connect to.
    origin: SimVec2,
    /// A structure producer's occupied cells `(left, bottom, right, top)`, inclusive; exits
    /// connect to the nearest of them. `None` for a producer that is not a structure.
    footprint: Option<(i64, i64, i64, i64)>,
    /// The ring rectangle `(left, bottom, right, top)` in cells, inclusive.
    bounds: (i64, i64, i64, i64),
    /// The product's collision radius in world units: its size against other bodies.
    radius: SimScalar,
    /// The product's terrain radius in world units: its disc against terrain, structures and
    /// the map edge (plan `2026-11-18-inner-radius` T7).
    terrain_radius: SimScalar,
    /// Whether the product flies, so ground connectivity does not apply and only air bodies block.
    airborne: bool,
}

/// The cells on the perimeter of `bounds` `(left, bottom, right, top)`, clockwise from the
/// minimum corner: the bottom row rightward, the right column upward, the top row leftward and
/// the left column downward, each corner visited once.
fn exit_ring_candidates(bounds: (i64, i64, i64, i64)) -> impl Iterator<Item = (i64, i64)> {
    (0..4).flat_map(move |side| {
        let (start, end) = if side % 2 == 0 {
            (bounds.0, bounds.2)
        } else {
            (bounds.1 + 1, bounds.3 - 1)
        };
        (start..=end).map(move |offset| match side {
            0 => (offset, bounds.1),
            1 => (bounds.2, offset),
            2 => (bounds.2 - (offset - bounds.0), bounds.3),
            _ => (bounds.0, bounds.3 - (offset - (bounds.1 + 1)) - 1),
        })
    })
}

#[cfg(test)]
mod tests {
    use super::exit_ring_candidates;
    use crate::movement::geometry::segment_overlaps_open_box;
    use sim_types::ShapeKind;

    #[test]
    fn exit_ring_is_walked_clockwise_from_the_minimum_corner_once_per_cell() {
        let order: Vec<_> = exit_ring_candidates((0, 0, 2, 2)).collect();
        assert_eq!(
            order,
            [
                (0, 0),
                (1, 0),
                (2, 0),
                (2, 1),
                (2, 2),
                (1, 2),
                (0, 2),
                (0, 1)
            ]
        );
        let wide: Vec<_> = exit_ring_candidates((-1, 3, 2, 7)).collect();
        assert_eq!(wide.len(), 2 * 4 + 2 * 3);
        assert_eq!(wide.first(), Some(&(-1, 3)));
        assert_eq!(wide.last(), Some(&(-1, 4)));
    }

    #[test]
    fn open_ground_exit_is_the_first_ring_cell_at_the_minimum_corner(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let map = sim_map::GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 1);
        let dome = |shape: &ShapeKind| matches!(shape, ShapeKind::BuildingDome { .. });
        let producer_type = world
            .catalog
            .archetypes()
            .find(|a| dome(&a.shape))
            .map(|a| a.unit_type_id)
            .ok_or("catalog has no structure")?;
        let (product, radius) = world
            .catalog
            .archetypes()
            .find(|a| !dome(&a.shape) && a.collision_radius > sim_types::SimScalar::ZERO)
            // The ring keeps the product's terrain radius off the footprint (D-8).
            .map(|a| (a.unit_type_id, a.terrain_radius))
            .ok_or("catalog has no mobile unit")?;
        let producer = world.spawn_structure(producer_type, sim_types::PlayerId(0), 10, 10)?;
        let (left, bottom) = world.map.structures()[&producer].top_left;
        world.rebuild_production_occupancy()?;
        let margin = (radius.raw() + (1_i64 << 31) + (1_i64 << 32) - 1) >> 32;
        let corner =
            |cell: i64| sim_types::SimScalar::from_raw(((cell - margin) << 32) + (1_i64 << 31));
        assert_eq!(
            world.production_exit(producer, product)?,
            Some(sim_types::SimVec2::new(
                corner(i64::from(left)),
                corner(i64::from(bottom))
            ))
        );
        Ok(())
    }

    /// T-8 (plan `2026-11-18-inner-radius` D-8): the ring keeps the product's terrain radius
    /// from the footprint, so a product of radius 0.625 and terrain radius 0.5 exits one cell
    /// off it (`ceil(0.5 + 0.5)`; two cells at `ceil(0.625 + 0.5)`), while bodies still meet it
    /// at its collision radius: a body of radius 0.375 at 0.95 from the first exit (below
    /// 0.625 + 0.375, above 0.5 + 0.375) moves the exit to the next cell.
    #[test]
    fn an_exit_ring_keeps_the_terrain_radius_from_the_footprint_and_the_collision_radius_from_bodies(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};
        let map = sim_map::GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 1);
        let dome = |shape: &ShapeKind| matches!(shape, ShapeKind::BuildingDome { .. });
        let producer_type = world
            .catalog
            .archetypes()
            .find(|a| dome(&a.shape))
            .map(|a| a.unit_type_id)
            .ok_or("catalog has no structure")?;
        let ground: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| {
                !dome(&a.shape)
                    && a.mover_type == MoverType::Ground
                    && a.max_speed > SimScalar::ZERO
            })
            .map(|a| a.unit_type_id)
            .take(2)
            .collect();
        let [product, blocker] = ground[..] else {
            return Err("two ground units".into());
        };
        for (kind, collision, terrain) in [(product, 0.625, 0.5), (blocker, 0.375, 0.375)] {
            let arch = world.catalog.get_mut(kind)?;
            arch.collision_radius = SimScalar::from_num(collision);
            arch.terrain_radius = SimScalar::from_num(terrain);
        }
        let producer = world.spawn_structure(producer_type, PlayerId(0), 10, 10)?;
        let (left, bottom) = world.map.structures()[&producer].top_left;
        let (left, bottom) = (f64::from(left), f64::from(bottom));
        let point = |x: f64, y: f64| SimVec2::new(SimScalar::from_num(x), SimScalar::from_num(y));
        world.rebuild_production_occupancy()?;
        assert_eq!(
            world.production_exit(producer, product)?,
            Some(point(left - 0.5, bottom - 0.5)),
            "one cell off the footprint"
        );
        world.spawn_unit(blocker, PlayerId(0), point(left - 0.5, bottom - 1.45))?;
        world.rebuild_production_occupancy()?;
        assert_eq!(
            world.production_exit(producer, product)?,
            Some(point(left + 0.5, bottom - 0.5)),
            "the body within the collision radii moves the exit along the ring"
        );
        Ok(())
    }

    /// Review F-2 (plan `2026-11-18-inner-radius` T7): at the map's corner the ring's corner
    /// cell (0, 0) has its centre 0.5 from both edges. A product of radius 0.625 and terrain
    /// radius 0.5 exits there: the map-bounds check and both egress checks use the terrain
    /// radius, and either one at the radius 0.625 would refuse the cell.
    #[test]
    fn an_exit_cell_at_the_map_corner_admits_the_terrain_disc(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};
        let map = sim_map::GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 2);
        let dome = |shape: &ShapeKind| matches!(shape, ShapeKind::BuildingDome { .. });
        let producer_type = world
            .catalog
            .archetypes()
            .find(|a| dome(&a.shape))
            .map(|a| a.unit_type_id)
            .ok_or("catalog has no structure")?;
        let product = world
            .catalog
            .archetypes()
            .find(|a| {
                !dome(&a.shape)
                    && a.mover_type == MoverType::Ground
                    && a.max_speed > SimScalar::ZERO
            })
            .map(|a| a.unit_type_id)
            .ok_or("a ground unit")?;
        let arch = world.catalog.get_mut(product)?;
        arch.collision_radius = SimScalar::from_num(0.625);
        arch.terrain_radius = SimScalar::from_num(0.5);
        let producer = world.spawn_structure(producer_type, PlayerId(0), 1, 1)?;
        world.rebuild_production_occupancy()?;
        let half = SimScalar::HALF;
        assert_eq!(
            world.production_exit(producer, product)?,
            Some(SimVec2::new(half, half))
        );
        Ok(())
    }

    /// S-5 (plan `2026-11-18-structure-contact` D-2 B4, B-131): a 3×3 producer's own circle
    /// (half its width, 1.5) no longer refuses the middle cell of its west face's exit ring. A
    /// product of radius 0.625 and terrain radius 0.5 has its ring one cell off the footprint;
    /// that cell's centre lies 0.5 from the face (at least the terrain radius, which egress
    /// checks) and 2.0 from the producer's centre, inside the old sum 1.5 + 0.625. A unit of
    /// radius 0.375 standing 0.9 from the centre (below 0.625 + 0.375) still refuses it.
    #[test]
    fn a_producers_circle_does_not_block_its_face_middle_exit(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};
        let map = sim_map::GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 3);
        let dome = |shape: &ShapeKind| matches!(shape, ShapeKind::BuildingDome { .. });
        let producer_type = world
            .catalog
            .archetypes()
            .find(|a| {
                matches!(
                    a.shape,
                    ShapeKind::BuildingDome {
                        footprint_w: 3,
                        footprint_h: 3,
                        ..
                    }
                )
            })
            .map(|a| a.unit_type_id)
            .ok_or("catalog has no 3x3 structure")?;
        let ground: Vec<_> = world
            .catalog
            .archetypes()
            .filter(|a| {
                !dome(&a.shape)
                    && a.mover_type == MoverType::Ground
                    && a.max_speed > SimScalar::ZERO
            })
            .map(|a| a.unit_type_id)
            .take(2)
            .collect();
        let [product, holder] = ground[..] else {
            return Err("two ground units".into());
        };
        for (kind, collision, terrain) in [(product, 0.625, 0.5), (holder, 0.375, 0.375)] {
            let arch = world.catalog.get_mut(kind)?;
            arch.collision_radius = SimScalar::from_num(collision);
            arch.terrain_radius = SimScalar::from_num(terrain);
        }
        let producer = world.spawn_structure(producer_type, PlayerId(0), 10, 10)?;
        let (left, bottom) = world.map.structures()[&producer].top_left;
        let (cell_x, cell_y) = (i64::from(left) - 1, i64::from(bottom) + 1);
        let centre = SimVec2::new(
            SimScalar::from_num(f64::from(left) - 0.5),
            SimScalar::from_num(f64::from(bottom) + 1.5),
        );
        world.rebuild_production_occupancy()?;
        let ring = world
            .exit_ring(producer, product)?
            .ok_or("the product fits the map")?;
        assert_eq!(
            world.free_exit_point(&ring, cell_x, cell_y)?,
            Some(centre),
            "the producer's circle does not refuse its face-middle exit"
        );
        world.spawn_unit(
            holder,
            PlayerId(0),
            SimVec2::new(centre.x - SimScalar::from_num(0.9), centre.y),
        )?;
        world.rebuild_production_occupancy()?;
        assert_eq!(
            world.free_exit_point(&ring, cell_x, cell_y)?,
            None,
            "a unit within the collision radii still occupies it"
        );
        Ok(())
    }

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
        map.place_structure(
            sim_types::EntityId(1),
            (2, 2),
            sim_map::StructureFootprint::square(1, 1),
        )?;
        let world = crate::test_support::world(map, 1);
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
