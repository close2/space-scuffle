//! Training countdown, completion and death transaction preparation.
//!
//! A ready front remains registered until a legal child is committed. Dead entities
//! retain ledger contributions until validated reap; no refund or resurrection occurs.

use super::{ProductionBlock, ProductionInvariant, STANDARD_SUPPLY_CAP};
use crate::{PlayerResources, SimWorld, SimulationDiagnostic};
use sim_types::{EntityId, ShapeKind, Tick};

impl SimWorld {
    pub(crate) fn advance_production(&mut self) {
        self.production.blocks.clear();
        self.production.ready.clear();
        if let Err(error) = self.validate_supply_ledger() {
            self.report_production_failure(error);
            return;
        }
        for entity in self.entities.values_mut() {
            if !entity.is_alive() || !entity.is_completed {
                continue;
            }
            if entity.production_queue.is_empty() {
                continue;
            }
            if let Err(source) = entity
                .production_queue
                .front()
                .ok_or(ProductionInvariant::EntityState {
                    entity: entity.id,
                    reason: "production source has no front job",
                })
                .and_then(|job| validate_job_source(&self.catalog, &self.map, entity, job))
            {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::ProductionInvariant {
                        tick: Tick(self.current_tick),
                        source,
                    });
                continue;
            }
            let Some(job) = entity.production_queue.front_mut() else {
                self.tick_diagnostics
                    .push(SimulationDiagnostic::ProductionInvariant {
                        tick: Tick(self.current_tick),
                        source: ProductionInvariant::EntityState {
                            entity: entity.id,
                            reason: "validated front disappeared before countdown",
                        },
                    });
                continue;
            };
            job.remaining_ticks = job.remaining_ticks.saturating_sub(1);
            if job.remaining_ticks == 0 {
                self.production.ready.push(entity.id);
            }
        }
    }

    pub(crate) fn complete_production(&mut self) {
        if self.production.ready.is_empty() {
            return;
        }
        if let Err(error) = self.rebuild_production_occupancy() {
            self.report_production_failure(error);
            return;
        }
        for index in 0..self.production.ready.len() {
            let producer = self.production.ready[index];
            let Some(entity) = self
                .entities
                .get(&producer)
                .filter(|e| e.is_alive() && e.is_completed)
            else {
                continue;
            };
            let Some(job) = entity.production_queue.front().copied() else {
                continue;
            };
            let owner = entity.owner;
            let result = self
                .production_exit(producer, job.product)
                .and_then(|position| {
                    position
                        .map(|pos| self.prepare_spawn(job.product, owner, pos, job.reserved_supply))
                        .transpose()
                });
            match result {
                Ok(Some(prepared)) => {
                    let position = prepared.entity.position;
                    let radius = prepared.radius;
                    let Some(source) = self.entities.get_mut(&producer) else {
                        self.report_production_failure(ProductionInvariant::EntityState {
                            entity: producer,
                            reason: "validated producer disappeared before completion commit",
                        });
                        continue;
                    };
                    if source.production_queue.front() != Some(&job) {
                        self.report_production_failure(ProductionInvariant::EntityState {
                            entity: producer,
                            reason: "validated front changed before completion commit",
                        });
                        continue;
                    }
                    // All fallible preparation is complete. Pop and insertion are one
                    // exclusive commit; no domain error can occur between these writes.
                    source.production_queue.pop_front();
                    let child = self.commit_spawn(prepared);
                    self.production.occupancy.insert(child, position);
                    self.production.max_radius = self.production.max_radius.max(radius);
                }
                Ok(None) => self
                    .production
                    .blocks
                    .push(ProductionBlock::ExitObstructed { producer }),
                Err(error) => self.report_production_failure(error),
            }
        }
    }

    /// Prepares one lifecycle release against the caller's cumulative group balance.
    /// The caller validates owner consistency and commits only after every member succeeds.
    pub(crate) fn prepare_reap_with_resources(
        &self,
        id: EntityId,
        resources: PlayerResources,
    ) -> Result<(PlayerResources, bool, sim_types::SimScalar), ProductionInvariant> {
        let entity = self
            .entities
            .get(&id)
            .ok_or(ProductionInvariant::EntityState {
                entity: id,
                reason: "reap victim disappeared",
            })?;
        let arch = self.catalog.get(entity.unit_type_id).map_err(|_| {
            ProductionInvariant::MissingArchetype {
                entity: id,
                unit_type: entity.unit_type_id,
            }
        })?;
        let mut released = entity.captured_supply_used;
        for job in &entity.production_queue {
            released = released.checked_add(job.reserved_supply).ok_or(
                ProductionInvariant::Arithmetic {
                    operation: "destroyed queue reservation sum",
                },
            )?;
        }
        let mut economy = resources;
        economy.supply_used =
            economy
                .supply_used
                .checked_sub(released)
                .ok_or(ProductionInvariant::Arithmetic {
                    operation: "reap supply release",
                })?;
        economy.supply_provided_total = economy
            .supply_provided_total
            .checked_sub(entity.captured_supply_provided)
            .ok_or(ProductionInvariant::Arithmetic {
                operation: "reap provider release",
            })?;
        economy.supply_max = economy.supply_provided_total.min(STANDARD_SUPPLY_CAP);
        let dome = matches!(arch.shape, ShapeKind::BuildingDome { .. });
        if dome {
            let placed =
                self.map
                    .structures()
                    .get(&id)
                    .ok_or(ProductionInvariant::EntityState {
                        entity: id,
                        reason: "destroyed structure foundation missing",
                    })?;
            if placed.width == 0
                || placed.height == 0
                || u64::from(placed.top_left.0) + u64::from(placed.width)
                    > u64::from(self.map.width())
                || u64::from(placed.top_left.1) + u64::from(placed.height)
                    > u64::from(self.map.height())
            {
                return Err(ProductionInvariant::EntityState {
                    entity: id,
                    reason: "destroyed foundation bounds invalid",
                });
            }
            for dy in 0..u32::from(placed.height) {
                for dx in 0..u32::from(placed.width) {
                    if !self
                        .map
                        .cell(placed.top_left.0 + dx, placed.top_left.1 + dy)
                        .is_ok_and(|cell| {
                            cell.occupying_structure == Some(id)
                                && cell
                                    .flags
                                    .contains(sim_map::CellFlags::OCCUPIED_BY_STRUCTURE)
                        })
                    {
                        return Err(ProductionInvariant::EntityState {
                            entity: id,
                            reason: "destroyed foundation occupancy mismatch",
                        });
                    }
                }
            }
        } else if self.map.structures().contains_key(&id) {
            return Err(ProductionInvariant::EntityState {
                entity: id,
                reason: "mobile entity owns a foundation",
            });
        }
        Ok((economy, dome, arch.collision_radius))
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one production transaction; splitting is plan step 9.2"
)]
pub(super) fn validate_job_source(
    catalog: &crate::UnitCatalog,
    map: &sim_map::GameMap,
    entity: &crate::Entity,
    job: &super::ProductionJob,
) -> Result<(), ProductionInvariant> {
    if entity.is_loaded_in_transport() {
        return Err(ProductionInvariant::EntityState {
            entity: entity.id,
            reason: "carried producer has a paid job",
        });
    }
    let source =
        catalog
            .get(entity.unit_type_id)
            .map_err(|_| ProductionInvariant::MissingArchetype {
                entity: entity.id,
                unit_type: entity.unit_type_id,
            })?;
    let target = catalog
        .get(job.product)
        .map_err(|_| ProductionInvariant::MissingArchetype {
            entity: entity.id,
            unit_type: job.product,
        })?;
    if target.supply_cost != job.reserved_supply
        || matches!(target.shape, ShapeKind::BuildingDome { .. })
    {
        return Err(ProductionInvariant::EntityState {
            entity: entity.id,
            reason: "queued product is unsupported or differs from reserved supply",
        });
    }
    for arch in [source, target] {
        if arch.collision_radius <= sim_types::SimScalar::ZERO {
            return Err(ProductionInvariant::InvalidDefinition {
                unit_type: arch.unit_type_id,
                field: "collision_radius",
            });
        }
    }
    let mut permits = false;
    for (index, slug) in source.trains_units.iter().enumerate() {
        let product = catalog
            .get_by_slug(slug)
            .ok_or(ProductionInvariant::InvalidDefinition {
                unit_type: source.unit_type_id,
                field: "trains_units reference",
            })?;
        if source.trains_units[..index].iter().any(|prior| {
            catalog
                .get_by_slug(prior)
                .is_some_and(|a| a.unit_type_id == product.unit_type_id)
        }) {
            return Err(ProductionInvariant::InvalidDefinition {
                unit_type: source.unit_type_id,
                field: "duplicate trains_units reference",
            });
        }
        permits |= product.unit_type_id == job.product;
    }
    if !permits {
        return Err(ProductionInvariant::EntityState {
            entity: entity.id,
            reason: "producer no longer permits its accepted product",
        });
    }
    if let ShapeKind::BuildingDome {
        footprint_w,
        footprint_h,
    } = source.shape
    {
        let placed = map
            .structures()
            .get(&entity.id)
            .ok_or(ProductionInvariant::EntityState {
                entity: entity.id,
                reason: "producer foundation absent",
            })?;
        if placed.width == 0
            || placed.height == 0
            || placed.width != footprint_w
            || placed.height != footprint_h
            || u64::from(placed.top_left.0) + u64::from(placed.width) > u64::from(map.width())
            || u64::from(placed.top_left.1) + u64::from(placed.height) > u64::from(map.height())
        {
            return Err(ProductionInvariant::EntityState {
                entity: entity.id,
                reason: "producer foundation dimensions mismatch",
            });
        }
        let center_x = (i128::from(placed.top_left.0) << 32) + (i128::from(placed.width) << 31);
        let center_y = (i128::from(placed.top_left.1) << 32) + (i128::from(placed.height) << 31);
        if i128::from(entity.position.x.raw()) != center_x
            || i128::from(entity.position.y.raw()) != center_y
        {
            return Err(ProductionInvariant::EntityState {
                entity: entity.id,
                reason: "producer position differs from its foundation center",
            });
        }
        for dy in 0..u32::from(placed.height) {
            for dx in 0..u32::from(placed.width) {
                if !map
                    .cell(placed.top_left.0 + dx, placed.top_left.1 + dy)
                    .is_ok_and(|c| {
                        c.occupying_structure == Some(entity.id)
                            && c.flags.contains(sim_map::CellFlags::OCCUPIED_BY_STRUCTURE)
                    })
                {
                    return Err(ProductionInvariant::EntityState {
                        entity: entity.id,
                        reason: "producer foundation occupancy mismatch",
                    });
                }
            }
        }
    } else {
        let r = i128::from(source.collision_radius.raw());
        for (position, extent) in [
            (entity.position.x, map.width()),
            (entity.position.y, map.height()),
        ] {
            if i128::from(position.raw()) - r < 0
                || i128::from(position.raw()) + r > (i128::from(extent) << 32)
            {
                return Err(ProductionInvariant::EntityState {
                    entity: entity.id,
                    reason: "mobile producer footprint outside map",
                });
            }
        }
    }
    Ok(())
}
