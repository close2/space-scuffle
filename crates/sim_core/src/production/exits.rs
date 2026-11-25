//! Local producer egress geometry and refreshed collision admission.
//!
//! Ring 0 is the footprint expanded by the product's terrain radius. Its cells are tried
//! clockwise from the minimum corner, or nearest the producer's rally point first (plan
//! `2026-11-23-production-queue` D-9). Every ring-0 ground exit connects to the actual producer
//! boundary at the product's terrain radius; only that producer's footprint is waived, on the
//! leg from the footprint to the exit, never terrain or another structure's footprint.
//!
//! When ring 0 has no free cell, outer rings one cell wider each are searched in the same order
//! until a ring lies wholly off the map (B-66): an outer cell needs legal ground for the
//! product's disc on the producer's cliff level and no body, but no connection, since the game
//! places units that "may become stuck" rather than holding the queue. The search visits every
//! map cell at most once, and only on a tick a ready producer's ring 0 is full;
//! `ExitObstructed` is published only when the whole map has no cell.
//!
//! Occupancy holds the live units, never registered structures: a structure is not a body
//! and meets the product only through its footprint, which both egress checks test at the
//! terrain radius (plan `2026-11-18-structure-contact` D-2 B4, B-131). Units meet the product
//! at the sum of the collision radii.

use super::ProductionInvariant;
use crate::SimWorld;
use sim_map::CliffLevel;
use sim_types::{CollisionClass, EntityId, ShapeKind, SimScalar, SimVec2, UnitTypeId};

impl SimWorld {
    /// Rebuilds the exit occupancy from the live units that are not hidden (carried, or inside a
    /// structure while they gather) and their largest collision radius. Registered structures
    /// are left out: they are not bodies (D-2 B4).
    pub(crate) fn rebuild_production_occupancy(&mut self) -> Result<(), ProductionInvariant> {
        self.production.occupancy.clear();
        self.production.max_radius = SimScalar::ZERO;
        for entity in self.entities.values().filter(|e| {
            e.is_alive() && !e.is_hidden() && !self.map.structures().contains_key(&e.id)
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

    /// Where a new `product` of `producer` emerges, a cell centre in world units, or `None`
    /// when no cell of the map can take it (plan `2026-11-23-production-queue` D-9, B-66).
    ///
    /// Ring 0 is the cells around the producer at the product's terrain radius, each tested as
    /// before (on the map, connected to the producer on the ground, clear of bodies). Only when
    /// ring 0 has no free cell are the outer rings searched, one cell wider each, until a ring
    /// lies wholly off the map; an outer cell needs no connection, only legal ground on the
    /// producer's level and no body. Within a ring, cells are tried clockwise from the minimum
    /// corner, or nearest the producer's rally point first when it has one.
    pub(crate) fn production_exit(
        &mut self,
        producer: EntityId,
        product: UnitTypeId,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        self.production.exit_cells_visited = 0;
        let Some(ring) = self.exit_ring(producer, product)? else {
            return Ok(None);
        };
        let rally = self.rally_point(producer);
        if let Some(point) = self.search_exit_ring(&ring, ring.bounds, rally, RingTest::Zero)? {
            return Ok(Some(point));
        }
        let level = if ring.airborne {
            None
        } else {
            Some(self.producer_level(&ring)?)
        };
        let (width, height) = (i64::from(self.map.width()), i64::from(self.map.height()));
        for step in 1_i64.. {
            let (left, bottom, right, top) = ring.bounds;
            let bounds = (left - step, bottom - step, right + step, top + step);
            // A ring that encloses the whole map has no cell on it: every cell was tried.
            if bounds.0 < 0 && bounds.1 < 0 && bounds.2 >= width && bounds.3 >= height {
                break;
            }
            if let Some(point) =
                self.search_exit_ring(&ring, bounds, rally, RingTest::Outer(level))?
            {
                return Ok(Some(point));
            }
        }
        Ok(None)
    }

    /// The first free cell of the ring `bounds`, in the order D-9 states: clockwise from the
    /// minimum corner, or by squared distance from the cell centre to `rally` and then clockwise
    /// index, each cell under the test `test` names.
    fn search_exit_ring(
        &mut self,
        ring: &ExitRing,
        bounds: (i64, i64, i64, i64),
        rally: Option<SimVec2>,
        test: RingTest,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        // PERFORMANCE RATIONALE:
        // Why: A ready producer whose exit is blocked retries every tick, and a rally orders
        // every ring it searches.
        // What: Stream the clockwise perimeter; with a rally, sort it in a retained buffer.
        // Trade-off: The buffer keeps the largest perimeter's capacity for the match.
        let mut order = std::mem::take(&mut self.production.exit_order);
        order.clear();
        if let Some(rally) = rally {
            order.extend(
                (0_u32..)
                    .zip(exit_ring_candidates(bounds))
                    .map(|(index, (x, y))| {
                        let dx = cell_centre_raw(x) - i128::from(rally.x.raw());
                        let dy = cell_centre_raw(y) - i128::from(rally.y.raw());
                        (dx * dx + dy * dy, index, x, y)
                    }),
            );
            order.sort_unstable_by_key(|&(distance, index, _, _)| (distance, index));
        } else {
            order.extend(
                (0_u32..)
                    .zip(exit_ring_candidates(bounds))
                    .map(|(index, (x, y))| (0, index, x, y)),
            );
        }
        let mut found = Ok(None);
        for &(_, _, x, y) in &order {
            let free = match test {
                RingTest::Zero => self.free_exit_point(ring, x, y),
                RingTest::Outer(level) => self.free_outer_exit_point(ring, x, y, level),
            };
            match free {
                Ok(None) => {}
                other => {
                    found = other;
                    break;
                }
            }
        }
        self.production.exit_order = order;
        found
    }

    /// The cliff level of the producer's ground: its footprint's minimum corner cell, or the
    /// cell under a mobile producer.
    fn producer_level(&self, ring: &ExitRing) -> Result<CliffLevel, ProductionInvariant> {
        let (x, y) = match ring.footprint {
            Some((left, bottom, _, _)) => (left, bottom),
            None => (ring.origin.x.raw() >> 32, ring.origin.y.raw() >> 32),
        };
        u32::try_from(x)
            .ok()
            .zip(u32::try_from(y).ok())
            .and_then(|(x, y)| self.map.cell(x, y).ok())
            .map(|cell| cell.cliff_level)
            .ok_or(ProductionInvariant::EntityState {
                entity: ring.producer,
                reason: "producer stands off the map",
            })
    }

    /// The centre of outer-ring cell `(x, y)` when the product may appear there (D-9 rule 3): on
    /// the map at its terrain radius; for a ground product, on legal ground at that disc and on
    /// the producer's `level`; and clear of every body of the product's layer.
    fn free_outer_exit_point(
        &mut self,
        ring: &ExitRing,
        x: i64,
        y: i64,
        level: Option<CliffLevel>,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        let Some(point) = self.exit_point_on_map(ring, x, y) else {
            return Ok(None);
        };
        if let Some(level) = level {
            let on_level = u32::try_from(x)
                .ok()
                .zip(u32::try_from(y).ok())
                .and_then(|(x, y)| self.map.cell(x, y).ok())
                .is_some_and(|cell| cell.cliff_level == level);
            if !on_level || !self.egress_connected(point, point, ring.terrain_radius, None) {
                return Ok(None);
            }
        }
        self.exit_point_clear(ring, point)
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
        let layer = crate::movement::MovementLayer::from_mover(target.mover_type);
        let airborne = layer == crate::movement::MovementLayer::Air;
        // A product is never in a mode: its class is its archetype's, else its layer's (plan
        // `2026-11-25-collision-classes` D-6).
        let class = target.collision_class.unwrap_or(if airborne {
            CollisionClass::AIR
        } else {
            CollisionClass::GROUND
        });
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
            class,
        }))
    }

    /// The centre of exit cell `(x, y)` when the product may emerge there, otherwise `None`.
    ///
    /// A free cell lies on the map with the product's terrain disc, connects to the producer's
    /// boundary on the ground at that disc, and overlaps no unit of a class the product's
    /// touches at the product's collision radius (structures are not in the occupancy; their footprints
    /// are the egress checks').
    fn free_exit_point(
        &mut self,
        ring: &ExitRing,
        x: i64,
        y: i64,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        let Some(point) = self.exit_point_on_map(ring, x, y) else {
            return Ok(None);
        };
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
        self.exit_point_clear(ring, point)
    }

    /// The centre of cell `(x, y)` in world units when the product's terrain disc fits the map
    /// there, counting the cell as visited (D-9 rule 4's bound); `None` off the map.
    fn exit_point_on_map(&mut self, ring: &ExitRing, x: i64, y: i64) -> Option<SimVec2> {
        if x < 0 || y < 0 || x >= i64::from(self.map.width()) || y >= i64::from(self.map.height()) {
            return None;
        }
        self.production.exit_cells_visited = self.production.exit_cells_visited.saturating_add(1);
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
            return None;
        }
        Some(point)
    }

    /// `Some(point)` when no unit whose collision class the product's touches (a burrowed body
    /// or one of another authored class does not; plan `2026-11-25-collision-classes` D-6)
    /// overlaps the product at `point` at the sum of their collision radii.
    fn exit_point_clear(
        &mut self,
        ring: &ExitRing,
        point: SimVec2,
    ) -> Result<Option<SimVec2>, ProductionInvariant> {
        let radius = ring.radius;
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
            let layer = crate::movement::MovementLayer::from_mover(arch.mover_type);
            if !ring
                .class
                .touches(crate::movement::collision_class_of(other, arch, layer))
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
    /// Whether the product flies, so ground connectivity does not apply.
    airborne: bool,
    /// The product's collision class: only occupants whose class it touches block an exit
    /// (plan `2026-11-25-collision-classes` D-6, B-212).
    class: CollisionClass,
}

/// Which test a ring's cells take (plan `2026-11-23-production-queue` D-9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RingTest {
    /// Ring 0: connected to the producer on the ground.
    Zero,
    /// An outer ring: on the producer's cliff level for a ground product (`None` for an air
    /// product, which needs only the map and the air bodies).
    Outer(Option<CliffLevel>),
}

/// The centre of cell coordinate `cell` in raw Q32.32 world units, widened to `i128`.
fn cell_centre_raw(cell: i64) -> i128 {
    (i128::from(cell) << 32) + (1_i128 << 31)
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

    /// A burrowed stance that changes nothing else, for A-10.
    fn burrowed_mode() -> sim_types::UnitModeSpec {
        use sim_types::SimScalar;
        sim_types::UnitModeSpec {
            mode_name: "exit_class_test_burrowed".into(),
            cast_ticks: 0,
            leave_cast_ticks: 0,
            cast_random_delay_max_ticks: SimScalar::ZERO,
            leave_random_delay_max_ticks: SimScalar::ZERO,
            is_immobile: true,
            unpushable: true,
            is_cloaked: false,
            burrowed: true,
            collision_class: None,
            mover_type_override: None,
            sight_radius_override: None,
            ground_weapon_override: None,
            air_weapon_override: None,
            energy_cost_per_second: SimScalar::ZERO,
            orders: sim_types::StanceOrders::ALL,
        }
    }

    /// A-10 (plan `2026-11-25-collision-classes` D-6, B-212): an exit cell is blocked only by an
    /// occupant whose collision class the product's touches. A ground product exits onto a
    /// burrowed body or a body of an authored class `x`, and not onto a ground body; an `x`
    /// product exits onto a ground or burrowed body and not onto an `x` body; an air product
    /// ignores them all.
    #[test]
    fn an_exit_cell_is_blocked_only_by_the_products_collision_class(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::SimWorld;
        use sim_types::{MoverType, PlayerId, SimScalar, SimVec2, UnitTypeId};
        let map = sim_map::GameMap::new(
            32,
            32,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 3);
        let x = world.catalog.author_collision_class_for_test("x");
        let dome = |shape: &ShapeKind| matches!(shape, ShapeKind::BuildingDome { .. });
        let producer_type = world
            .catalog
            .archetypes()
            .find(|a| dome(&a.shape))
            .map(|a| a.unit_type_id)
            .ok_or("catalog has no structure")?;
        let mobile = |mover: MoverType| {
            world
                .catalog
                .archetypes()
                .find(|a| !dome(&a.shape) && a.mover_type == mover && a.max_speed > SimScalar::ZERO)
                .cloned()
                .ok_or("a mobile unit of the layer")
        };
        let (ground_base, air_base) = (mobile(MoverType::Ground)?, mobile(MoverType::Air)?);
        let free = |world: &SimWorld, after: u16| {
            (after..=u16::MAX)
                .map(UnitTypeId)
                .find(|id| world.catalog.get(*id).is_err())
                .ok_or("a free unit type id")
        };
        let mut register = |base: &crate::UnitArchetype,
                            class: Option<sim_types::CollisionClass>,
                            burrowed: bool|
         -> Result<UnitTypeId, Box<dyn std::error::Error>> {
            let mut arch = base.clone();
            arch.unit_type_id = free(&world, 200)?;
            arch.slug = format!("exit_class_test_{}", arch.unit_type_id.0);
            arch.collision_radius = SimScalar::HALF;
            arch.terrain_radius = SimScalar::HALF;
            arch.collision_class = class;
            arch.modes.clear();
            if burrowed {
                arch.modes.push(burrowed_mode());
            }
            let id = arch.unit_type_id;
            world.catalog.register(arch);
            Ok(id)
        };
        let ground = register(&ground_base, None, false)?;
        let burrower = register(&ground_base, None, true)?;
        let tall = register(&ground_base, Some(x), false)?;
        let air = register(&air_base, None, false)?;
        let producer = world.spawn_structure(producer_type, PlayerId(0), 10, 10)?;
        let (left, bottom) = world.map.structures()[&producer].top_left;
        let first = SimVec2::new(
            SimScalar::from_num(f64::from(left) - 0.5),
            SimScalar::from_num(f64::from(bottom) - 0.5),
        );
        world.rebuild_production_occupancy()?;
        for product in [ground, tall, air] {
            assert_eq!(
                world.production_exit(producer, product)?,
                Some(first),
                "open ring"
            );
        }
        // One occupant on the first exit cell at a time.
        let cases = [
            (burrower, true, [true, true, true]),
            (tall, false, [true, false, true]),
            (ground, false, [false, true, true]),
        ];
        for (occupant_type, burrowed, open) in cases {
            let occupant = world.spawn_unit(occupant_type, PlayerId(1), first)?;
            if burrowed {
                world
                    .entities
                    .get_mut(&occupant)
                    .ok_or("occupant")?
                    .active_mode = Some("exit_class_test_burrowed".into());
            }
            world.rebuild_production_occupancy()?;
            for (product, expect_first) in [ground, tall, air].into_iter().zip(open) {
                let exit = world.production_exit(producer, product)?;
                assert_eq!(
                    exit == Some(first),
                    expect_first,
                    "occupant {occupant_type:?} (burrowed {burrowed}), product {product:?}: {exit:?}"
                );
            }
            // Out of the way of the next case, in the map's far corner.
            world
                .entities
                .get_mut(&occupant)
                .ok_or("occupant")?
                .position = SimVec2::new(SimScalar::from_num(30), SimScalar::from_num(30));
        }
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

    /// P-6 (plan `2026-11-23-production-queue` D-9 rule 4): with a body on every cell of a small
    /// map, the search ends with no exit after examining every map cell outside ring 0's interior
    /// exactly once (rings are disjoint perimeters, so the count is that number only if no cell
    /// is examined twice), and it stops at the ring that encloses the map.
    #[test]
    fn a_full_map_publishes_exit_obstructed_after_one_visit_per_cell(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use sim_types::{MoverType, PlayerId, SimScalar, SimVec2};
        const SIDE: u32 = 12;
        let map = sim_map::GameMap::new(
            SIDE,
            SIDE,
            sim_map::MapCell::default_ground(sim_map::CliffLevel(0)),
        )?;
        let mut world = crate::test_support::world(map, 4);
        let dome = |shape: &ShapeKind| matches!(shape, ShapeKind::BuildingDome { .. });
        let producer_type = world
            .catalog
            .archetypes()
            .filter(|a| dome(&a.shape))
            .min_by_key(|a| {
                a.shape
                    .footprint()
                    .map(|(w, h)| u32::from(w) * u32::from(h))
            })
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
        arch.collision_radius = SimScalar::from_num(0.375);
        arch.terrain_radius = SimScalar::from_num(0.375);
        let producer = world.spawn_structure(producer_type, PlayerId(0), 3, 4)?;
        for y in 0..SIDE {
            for x in 0..SIDE {
                if world.map.cell(x, y)?.occupying_structure.is_none() {
                    let centre = SimVec2::new(
                        SimScalar::from_num(x) + SimScalar::HALF,
                        SimScalar::from_num(y) + SimScalar::HALF,
                    );
                    world.spawn_unit(product, PlayerId(1), centre)?;
                }
            }
        }
        world.rebuild_production_occupancy()?;
        assert_eq!(world.production_exit(producer, product)?, None);
        let ring = world
            .exit_ring(producer, product)?
            .ok_or("the product fits the map")?;
        let (left, bottom, right, top) = ring.bounds;
        let side = i64::from(SIDE);
        let interior = (0..side)
            .flat_map(|y| (0..side).map(move |x| (x, y)))
            .filter(|&(x, y)| x > left && x < right && y > bottom && y < top)
            .count();
        let expected = SIDE * SIDE - u32::try_from(interior)?;
        assert_eq!(world.production.exit_cells_visited, expected);
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
