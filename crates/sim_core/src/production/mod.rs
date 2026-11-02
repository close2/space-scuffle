//! Atomic capability-driven training, research, construction and supply accounting.
//!
//! Authored catalogs authorize products. Commands pay and reserve full FIFO supply at
//! admission; completion transfers that reservation into a registered entity, or into a
//! researched upgrade. Cancelling a queued job returns exactly what its admission took.
//! Logical failures occur before authoritative writes. Scratch/output buffers are
//! derived, reusable and excluded from checksums; clients must consume outcomes.

pub mod construction;
mod exits;
mod lifecycle;
mod research;

use crate::upgrades::UpgradeId;
use crate::{Entity, PlayerResources, SimWorld, SpatialHashGrid, UnitArchetype, SPATIAL_CELL_SIZE};
use sim_types::{EntityId, PlayerId, ShapeKind, SimScalar, SimVec2, Tick, UnitTypeId};
use thiserror::Error;

/// Standard effective supply limit; provider totals retain values above this cap.
pub const STANDARD_SUPPLY_CAP: u32 = 200;

/// What a queued job delivers when its countdown ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionItem {
    /// A mobile unit of this archetype, emerging at a free exit.
    Unit(UnitTypeId),
    /// This catalog upgrade, applied to the owner.
    Research(UpgradeId),
}

/// Accepted FIFO job with a lossless 25 Hz countdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionJob {
    /// Catalog identity selected at admission.
    pub item: ProductionItem,
    /// Remaining production updates; zero means ready, possibly awaiting an exit.
    pub remaining_ticks: u32,
    /// Countdown length at admission, the denominator of published progress.
    pub total_ticks: u32,
    /// Full supply captured and reserved at admission, including waiting jobs.
    pub reserved_supply: u32,
    /// Minerals taken at admission, which cancelling returns.
    pub paid_minerals: u32,
    /// Gas taken at admission, which cancelling returns.
    pub paid_gas: u32,
}

/// Corrupt state or unrepresentable arithmetic, never an ordinary player rejection.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProductionInvariant {
    /// A malformed carrier relation invalidates an independently requested operation.
    #[error(transparent)]
    Transport(#[from] crate::transport::TransportInvariant),
    /// An existing entity references an invalid player slot.
    #[error("invalid lifecycle player {player:?}")]
    InvalidPlayer {
        /// Invalid owner.
        player: PlayerId,
    },
    /// A registered entity's catalog identity cannot be resolved.
    #[error("entity {entity:?} references missing catalog identity {unit_type:?}")]
    MissingArchetype {
        /// Existing entity.
        entity: EntityId,
        /// Missing identity.
        unit_type: UnitTypeId,
    },
    /// A mutable definition violates the operation's representation contract.
    #[error("invalid production definition {unit_type:?}: {field}")]
    InvalidDefinition {
        /// Affected definition.
        unit_type: UnitTypeId,
        /// Failed field or contract.
        field: &'static str,
    },
    /// A checked integer operation cannot represent its result.
    #[error("production arithmetic overflow: {operation}")]
    Arithmetic {
        /// Operation in simulation units.
        operation: &'static str,
    },
    /// The public economy differs from registered captured contributions.
    #[error("supply ledger mismatch for {player:?}")]
    LedgerMismatch {
        /// Affected player.
        player: PlayerId,
    },
    /// Allocation cannot preserve a representable next-ID counter.
    #[error("entity ID allocator exhausted")]
    IdExhausted,
    /// Allocation would replace a registered entity or foundation.
    #[error("entity ID {entity:?} is already registered")]
    IdCollision {
        /// Conflicting candidate.
        entity: EntityId,
    },
    /// A producer, queue or foundation no longer satisfies its lifecycle contract.
    #[error("entity {entity:?}: {reason}")]
    EntityState {
        /// Affected entity.
        entity: EntityId,
        /// Invariant failure.
        reason: &'static str,
    },
}

/// Normal command rejection or explicit authoritative corruption.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProductionAdmissionError {
    /// Carried actors cannot acquire paid world-space production jobs.
    #[error("production recipient {0:?} is carried")]
    LoadedRecipient(EntityId),
    /// Player slot is outside the world's supported slots.
    #[error("unknown player {0:?}")]
    InvalidPlayer(PlayerId),
    /// Requested product is not present in the selected catalog.
    #[error("unknown production product {0:?}")]
    UnknownProduct(UnitTypeId),
    /// No entity was selected.
    #[error("select a producer")]
    EmptySelection,
    /// A selected entity does not exist.
    #[error("selected entity {0:?} does not exist")]
    MissingRecipient(EntityId),
    /// A selected entity is not owned by the issuing player.
    #[error("selected entity {0:?} is not owned by this player")]
    ForeignRecipient(EntityId),
    /// Dead selections cannot issue production commands.
    #[error("selected entity {0:?} is destroyed")]
    DeadRecipient(EntityId),
    /// None of the valid selections is completed and permits this product.
    #[error("no selected completed producer permits this product")]
    NoEligibleProducer,
    /// Dome morph/construction is not mobile training.
    #[error("product {0:?} requires a different production workflow")]
    UnsupportedProductionWorkflow(UnitTypeId),
    /// An actual completed, alive owned prerequisite structure is absent.
    #[error("missing prerequisite structure {0:?}")]
    MissingPrerequisite(UnitTypeId),
    /// Minerals are insufficient; no resource is charged.
    #[error("insufficient minerals: need {required}, have {available}")]
    Minerals {
        /// Required minerals.
        required: u32,
        /// Current minerals.
        available: u32,
    },
    /// Gas is insufficient; no resource is charged.
    #[error("insufficient gas: need {required}, have {available}")]
    Gas {
        /// Required gas.
        required: u32,
        /// Current gas.
        available: u32,
    },
    /// Full enqueue reservation would exceed capacity.
    #[error("insufficient supply: used {used}, requested {required}, capacity {capacity}")]
    Supply {
        /// Registered usage and reservations.
        used: u32,
        /// New reservation.
        required: u32,
        /// Effective capacity.
        capacity: u32,
    },
    /// Requested upgrade is not present in the selected catalog.
    #[error("unknown upgrade {0}")]
    UnknownUpgrade(UpgradeId),
    /// None of the valid selections is a completed structure researching this upgrade.
    #[error("no selected completed structure researches upgrade {0}")]
    NoEligibleResearcher(UpgradeId),
    /// The player already owns this upgrade; researching it again would double its effects.
    #[error("upgrade {0} is already researched")]
    UpgradeAlreadyResearched(UpgradeId),
    /// The player already has this upgrade in some queue.
    #[error("upgrade {0} is already being researched")]
    UpgradeAlreadyQueued(UpgradeId),
    /// A tiered upgrade needs its preceding tier first.
    #[error("upgrade {0} requires its preceding tier")]
    UpgradeTierLocked(UpgradeId),
    /// Corrupt authoritative state; consumers must stop/report the diagnostic.
    #[error(transparent)]
    Invariant(#[from] ProductionInvariant),
}

/// Evidence of exactly one committed training admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductionReceipt {
    /// Deterministically selected producer.
    pub producer: EntityId,
    /// Exact job appended after payment and reservation.
    pub job: ProductionJob,
}

/// Derived result of a Produce command, in input-vector order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductionOutcome {
    /// Tick in which input was processed.
    pub tick: Tick,
    /// Command's index in the authoritative bundle.
    pub command_index: usize,
    /// Issuing player.
    pub player: PlayerId,
    /// Admission receipt or reason; ordinary rejections do not stop simulation.
    pub result: Result<ProductionReceipt, ProductionAdmissionError>,
}

/// Current nonfatal obstruction observed for a ready producer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProductionBlock {
    /// No connected, unoccupied local exit can fit this product.
    ExitObstructed {
        /// Waiting producer.
        producer: EntityId,
    },
}

pub(crate) struct ProductionWorkspace {
    pub outcomes: Vec<ProductionOutcome>,
    pub blocks: Vec<ProductionBlock>,
    pub ready: Vec<EntityId>,
    pub occupancy: SpatialHashGrid,
    pub neighbors: Vec<EntityId>,
    pub max_radius: SimScalar,
}

impl ProductionWorkspace {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            outcomes: Vec::new(),
            blocks: Vec::new(),
            ready: Vec::new(),
            occupancy: SpatialHashGrid::new(width as usize, height as usize, SPATIAL_CELL_SIZE),
            neighbors: Vec::new(),
            max_radius: SimScalar::ZERO,
        }
    }
}

pub(crate) struct PreparedSpawn {
    pub entity: Entity,
    pub entity_shape: ShapeKind,
    pub radius: SimScalar,
    next_id: u32,
    economy: PlayerResources,
}

impl SimWorld {
    /// Checks registered captured supply, including dead entities awaiting reap.
    /// Public direct mutation must preserve this invariant; no silent reconciliation occurs.
    ///
    /// # Errors
    /// Returns typed player, ledger or arithmetic corruption.
    pub fn validate_supply_ledger(&self) -> Result<(), ProductionInvariant> {
        let mut used = [0_u64; 8];
        let mut provided = [0_u64; 8];
        for entity in self.entities.values() {
            let index = usize::from(entity.owner.0);
            if index >= used.len() {
                return Err(ProductionInvariant::InvalidPlayer {
                    player: entity.owner,
                });
            }
            used[index] = used[index]
                .checked_add(u64::from(entity.captured_supply_used))
                .ok_or(ProductionInvariant::Arithmetic {
                    operation: "registered supply total",
                })?;
            provided[index] = provided[index]
                .checked_add(u64::from(entity.captured_supply_provided))
                .ok_or(ProductionInvariant::Arithmetic {
                    operation: "provider total",
                })?;
            if !entity.is_completed && entity.captured_supply_provided != 0 {
                return Err(ProductionInvariant::EntityState {
                    entity: entity.id,
                    reason: "incomplete entity retains completed provider contribution",
                });
            }
            for job in &entity.production_queue {
                used[index] = used[index]
                    .checked_add(u64::from(job.reserved_supply))
                    .ok_or(ProductionInvariant::Arithmetic {
                        operation: "queued supply total",
                    })?;
            }
        }
        for (index, resources) in self.player_resources.iter().enumerate() {
            if used[index] != u64::from(resources.supply_used)
                || provided[index] != u64::from(resources.supply_provided_total)
                || resources.supply_max != resources.supply_provided_total.min(STANDARD_SUPPLY_CAP)
            {
                return Err(ProductionInvariant::LedgerMismatch {
                    player: PlayerId(u8::try_from(index).unwrap_or(u8::MAX)),
                });
            }
        }
        Ok(())
    }

    pub(crate) fn prepare_spawn(
        &self,
        unit_type: UnitTypeId,
        owner: PlayerId,
        pos: SimVec2,
        reserved: u32,
    ) -> Result<PreparedSpawn, ProductionInvariant> {
        let Some(resources) = self.player_resources.get(usize::from(owner.0)) else {
            return Err(ProductionInvariant::InvalidPlayer { player: owner });
        };
        let id = EntityId(self.next_entity_id);
        let arch =
            self.catalog
                .get(unit_type)
                .map_err(|_| ProductionInvariant::MissingArchetype {
                    entity: id,
                    unit_type,
                })?;
        let next_id = self
            .next_entity_id
            .checked_add(1)
            .ok_or(ProductionInvariant::IdExhausted)?;
        if self.entities.contains_key(&id) || self.map.structures().contains_key(&id) {
            return Err(ProductionInvariant::IdCollision { entity: id });
        }
        let hp_delta = self.player_upgrades[usize::from(owner.0)]
            .unit_stat_modifiers
            .get(&arch.slug)
            .map_or(SimScalar::ZERO, |m| m.max_hp_delta);
        let hp = arch
            .max_hp
            .raw()
            .checked_add(hp_delta.raw())
            .filter(|hp| *hp > 0)
            .ok_or(ProductionInvariant::InvalidDefinition {
                unit_type,
                field: "effective max_hp",
            })?;
        if arch.collision_radius <= SimScalar::ZERO {
            return Err(ProductionInvariant::InvalidDefinition {
                unit_type,
                field: "collision_radius",
            });
        }
        let mut economy = *resources;
        economy.supply_used = economy
            .supply_used
            .checked_sub(reserved)
            .and_then(|value| value.checked_add(arch.supply_cost))
            .ok_or(ProductionInvariant::Arithmetic {
                operation: "spawn supply transfer",
            })?;
        economy.supply_provided_total = economy
            .supply_provided_total
            .checked_add(arch.supply_provided)
            .ok_or(ProductionInvariant::Arithmetic {
                operation: "spawn provider total",
            })?;
        economy.supply_max = economy.supply_provided_total.min(STANDARD_SUPPLY_CAP);
        let mut entity = Entity::from_archetype(id, owner, pos, arch);
        entity.max_hp = SimScalar::from_raw(hp);
        entity.hp = entity.max_hp;
        Ok(PreparedSpawn {
            entity,
            entity_shape: arch.shape,
            radius: arch.collision_radius,
            next_id,
            economy,
        })
    }

    pub(crate) fn commit_spawn(&mut self, prepared: PreparedSpawn) -> EntityId {
        let id = prepared.entity.id;
        self.player_resources[usize::from(prepared.entity.owner.0)] = prepared.economy;
        self.next_entity_id = prepared.next_id;
        self.entities.insert(id, prepared.entity);
        id
    }

    /// Admits one job to the first completed selected producer whose authored list permits it.
    /// All selections must exist, be alive and owned. Full FIFO supply is reserved immediately.
    ///
    /// # Errors
    /// Rejections preserve authoritative state. Corruption additionally emits a fatal diagnostic.
    pub fn try_queue_production(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        product: UnitTypeId,
    ) -> Result<ProductionReceipt, ProductionAdmissionError> {
        if let Err(source) = self.validate_transport_links() {
            let error = ProductionInvariant::Transport(source);
            self.report_production_failure(error.clone());
            return Err(error.into());
        }
        self.queue_production_prevalidated(player, recipients, product)
    }

    // Public wrapper or normal tick validated relations; admission forbids passenger work.
    pub(crate) fn queue_production_prevalidated(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        product: UnitTypeId,
    ) -> Result<ProductionReceipt, ProductionAdmissionError> {
        let admitted = self.prepare_admission(player, recipients, product);
        self.commit_job(player, admitted)
    }

    fn prepare_admission(
        &self,
        player: PlayerId,
        recipients: &[EntityId],
        product: UnitTypeId,
    ) -> Result<(ProductionReceipt, PlayerResources), ProductionAdmissionError> {
        let Some(resources) = self.player_resources.get(usize::from(player.0)) else {
            return Err(ProductionAdmissionError::InvalidPlayer(player));
        };
        let arch = self
            .catalog
            .get(product)
            .map_err(|_| ProductionAdmissionError::UnknownProduct(product))?;
        if matches!(arch.shape, ShapeKind::BuildingDome { .. }) {
            return Err(ProductionAdmissionError::UnsupportedProductionWorkflow(
                product,
            ));
        }
        let producer = self
            .select_producer(player, recipients, |source| {
                let mut permits = false;
                for slug in &source.trains_units {
                    let target = self.catalog.get_by_slug(slug).ok_or(
                        ProductionInvariant::InvalidDefinition {
                            unit_type: source.unit_type_id,
                            field: "trains_units reference",
                        },
                    )?;
                    permits |= target.unit_type_id == product;
                }
                Ok(permits)
            })?
            .ok_or(ProductionAdmissionError::NoEligibleProducer)?;
        self.validate_supply_ledger()?;
        let job = ProductionJob {
            item: ProductionItem::Unit(product),
            remaining_ticks: arch.build_time_ticks,
            total_ticks: arch.build_time_ticks,
            reserved_supply: arch.supply_cost,
            paid_minerals: arch.mineral_cost,
            paid_gas: arch.gas_cost,
        };
        let source = self
            .entities
            .get(&producer)
            .ok_or(ProductionInvariant::EntityState {
                entity: producer,
                reason: "selected producer disappeared during preparation",
            })?;
        lifecycle::validate_job_source(
            &self.catalog,
            &self.upgrade_catalog,
            &self.map,
            source,
            &job,
        )?;
        if let Some(missing) =
            self.missing_prerequisite(player, product, &arch.requires_structures)?
        {
            return Err(ProductionAdmissionError::MissingPrerequisite(missing));
        }
        let economy = Self::charge_job(resources, &job)?;
        Ok((ProductionReceipt { producer, job }, economy))
    }

    /// Validates every selected recipient and returns the first completed one `permits` accepts.
    ///
    /// All selections must exist, be alive, owned and in the world, so one bad identity refuses
    /// the whole command whichever position it holds in the selection.
    fn select_producer(
        &self,
        player: PlayerId,
        recipients: &[EntityId],
        permits: impl Fn(&UnitArchetype) -> Result<bool, ProductionInvariant>,
    ) -> Result<Option<EntityId>, ProductionAdmissionError> {
        if recipients.is_empty() {
            return Err(ProductionAdmissionError::EmptySelection);
        }
        let mut chosen = None;
        for id in recipients {
            let entity = self
                .entities
                .get(id)
                .ok_or(ProductionAdmissionError::MissingRecipient(*id))?;
            if entity.is_loaded_in_transport() {
                return Err(ProductionAdmissionError::LoadedRecipient(*id));
            }
            if entity.owner != player {
                return Err(ProductionAdmissionError::ForeignRecipient(*id));
            }
            if !entity.is_alive() {
                return Err(ProductionAdmissionError::DeadRecipient(*id));
            }
            let source = self.catalog.get(entity.unit_type_id).map_err(|_| {
                ProductionInvariant::MissingArchetype {
                    entity: *id,
                    unit_type: entity.unit_type_id,
                }
            })?;
            let permitted = permits(source)?;
            if chosen.is_none() && entity.is_completed && permitted {
                chosen = Some(*id);
            }
        }
        Ok(chosen)
    }

    /// The first of `slugs` for which `player` has no completed structure, if any.
    pub(crate) fn missing_prerequisite(
        &self,
        player: PlayerId,
        subject: UnitTypeId,
        slugs: &[String],
    ) -> Result<Option<UnitTypeId>, ProductionInvariant> {
        for (index, slug) in slugs.iter().enumerate() {
            let required =
                self.catalog
                    .get_by_slug(slug)
                    .ok_or(ProductionInvariant::InvalidDefinition {
                        unit_type: subject,
                        field: "requires_structures reference",
                    })?;
            if slugs[..index].iter().any(|prior| {
                self.catalog
                    .get_by_slug(prior)
                    .is_some_and(|a| a.unit_type_id == required.unit_type_id)
            }) {
                return Err(ProductionInvariant::InvalidDefinition {
                    unit_type: subject,
                    field: "duplicate prerequisite reference",
                });
            }
            if !matches!(required.shape, ShapeKind::BuildingDome { .. }) {
                return Err(ProductionInvariant::InvalidDefinition {
                    unit_type: subject,
                    field: "prerequisite is not a structure",
                });
            }
            let mut found = false;
            for entity in self.entities.values().filter(|e| {
                e.owner == player && e.is_alive() && e.is_completed && !e.is_loaded_in_transport()
            }) {
                self.catalog.get(entity.unit_type_id).map_err(|_| {
                    ProductionInvariant::MissingArchetype {
                        entity: entity.id,
                        unit_type: entity.unit_type_id,
                    }
                })?;
                found |= entity.unit_type_id == required.unit_type_id
                    && self.map.structures().contains_key(&entity.id);
            }
            if !found {
                return Ok(Some(required.unit_type_id));
            }
        }
        Ok(None)
    }

    /// The player's economy after paying for `job`, or the shortage that refuses it.
    fn charge_job(
        resources: &PlayerResources,
        job: &ProductionJob,
    ) -> Result<PlayerResources, ProductionAdmissionError> {
        if resources.minerals < job.paid_minerals {
            return Err(ProductionAdmissionError::Minerals {
                required: job.paid_minerals,
                available: resources.minerals,
            });
        }
        if resources.gas < job.paid_gas {
            return Err(ProductionAdmissionError::Gas {
                required: job.paid_gas,
                available: resources.gas,
            });
        }
        let used = resources
            .supply_used
            .checked_add(job.reserved_supply)
            .ok_or(ProductionInvariant::Arithmetic {
                operation: "enqueue supply",
            })?;
        if job.reserved_supply > 0 && used > resources.supply_max {
            return Err(ProductionAdmissionError::Supply {
                used: resources.supply_used,
                required: job.reserved_supply,
                capacity: resources.supply_max,
            });
        }
        let mut economy = *resources;
        economy.minerals -= job.paid_minerals;
        economy.gas -= job.paid_gas;
        economy.supply_used = used;
        Ok(economy)
    }

    /// Commits an admitted job and its payment together.
    fn commit_job(
        &mut self,
        player: PlayerId,
        admitted: Result<(ProductionReceipt, PlayerResources), ProductionAdmissionError>,
    ) -> Result<ProductionReceipt, ProductionAdmissionError> {
        match admitted {
            Ok((receipt, economy)) => {
                // Exclusive world access preserves the validated recipient through commit.
                let Some(producer) = self.entities.get_mut(&receipt.producer) else {
                    let error = ProductionInvariant::EntityState {
                        entity: receipt.producer,
                        reason: "validated recipient disappeared before admission commit",
                    };
                    self.report_production_failure(error.clone());
                    return Err(error.into());
                };
                producer.production_queue.push_back(receipt.job);
                self.player_resources[usize::from(player.0)] = economy;
                Ok(receipt)
            }
            Err(error) => {
                if let ProductionAdmissionError::Invariant(source) = &error {
                    self.report_production_failure(source.clone());
                }
                Err(error)
            }
        }
    }

    /// Derived command outcomes; callers consume once or inspect until the next full tick.
    #[must_use]
    pub fn production_outcomes(&self) -> &[ProductionOutcome] {
        &self.production.outcomes
    }
    /// Drains command evidence while retaining buffer capacity.
    pub fn drain_production_outcomes(&mut self) -> std::vec::Drain<'_, ProductionOutcome> {
        self.production.outcomes.drain(..)
    }
    /// Current ready-producer obstructions, regenerated each production phase.
    #[must_use]
    pub fn production_blocks(&self) -> &[ProductionBlock] {
        &self.production.blocks
    }
    pub(crate) fn report_production_failure(&mut self, source: ProductionInvariant) {
        self.tick_diagnostics
            .push(crate::SimulationDiagnostic::ProductionInvariant {
                tick: Tick(self.current_tick),
                source,
            });
    }
}
