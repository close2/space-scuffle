//! Structure construction: site admission, foundations and their completion.
//!
//! # The life of a building
//! 1. **Order.** `Build` is admitted against cost, tech, a capable builder and the site, all
//!    before anything changes. Nothing is paid yet; two orders may promise the same minerals.
//! 2. **Walk.** The builder walks to the edge of the site.
//! 3. **Payment.** On arrival every check is repeated, and the foundation appears in one
//!    transaction with its payment. This is the moment resources are spent. If the site or
//!    the money has gone, the order ends with a [`TaskEventKind::BuildAbandoned`] event.
//! 4. **Raising.** The foundation's countdown advances one tick for each tick a builder works
//!    at its edge, however many are there. Hit points rise with it from a tenth of the
//!    maximum, and damage taken stays taken. Shields, supply, weapons and production wait for
//!    completion.
//!
//! # Interruptions
//! - **Builder lost or called away:** construction pauses. The foundation keeps its progress,
//!   stays damageable and counts as a structure, and any capable unit resumes it with
//!   `Construct`.
//! - **`CancelConstruction`:** the foundation is removed through the normal death path and
//!   [`CONSTRUCTION_CANCEL_REFUND_PERCENT`] of its payment returns.
//! - **Foundation destroyed:** nothing is refunded, and its builder's order ends.
//!
//! # Invariants
//! - A foundation contributes no supply until it completes, so the supply ledger only ever
//!   counts finished providers.
//! - Admission and arrival run the same [`SimWorld::check_build`], so they cannot disagree.
//! - Sites never cover a resource node, except an extractor, which must cover a gas node.

use super::{PreparedSpawn, ProductionInvariant, STANDARD_SUPPLY_CAP};
use crate::economy::{Footprint, GameCommandError, GameCommandReceipt, TaskEventKind};
use crate::SimWorld;
use sim_map::PlacementFailureReason;
use sim_types::{
    AnimState, EntityId, PlayerId, ResourceKind, ShapeKind, SimScalar, Tick, UnitOrder, UnitTypeId,
};
use thiserror::Error;

/// Share of a foundation's payment that `CancelConstruction` returns, rounded down.
pub const CONSTRUCTION_CANCEL_REFUND_PERCENT: u32 = 75;

/// A foundation starts at one part in this many of its maximum hit points.
const FOUNDATION_HP_DIVISOR: i32 = 10;

/// Ticks a builder waits at its site for bodies to leave before giving the order up.
const SITE_PATIENCE_TICKS: u16 = 50;

/// An unfinished building's construction record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Construction {
    /// Worked 25 Hz ticks still needed.
    pub remaining_ticks: u32,
    /// Total worked ticks, from the catalog build time at placement.
    pub total_ticks: u32,
    /// Hit points gained per worked tick.
    pub hp_per_tick: SimScalar,
    /// Hit points the final worked tick adds beyond `hp_per_tick` to absorb rounding.
    pub final_hp_residue: SimScalar,
    /// Minerals paid at placement, the base of a cancellation refund.
    pub paid_minerals: u32,
    /// Gas paid at placement, the base of a cancellation refund.
    pub paid_gas: u32,
    /// Latest tick a builder advanced this foundation, so two builders cannot double its rate.
    pub last_worked_tick: Option<Tick>,
}

/// Why a structure cannot be started, at admission or on the builder's arrival.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BuildRejection {
    /// The identity is not in the catalog.
    #[error("unknown structure {0:?}")]
    UnknownStructure(UnitTypeId),
    /// The identity is a mobile unit, which `Produce` trains.
    #[error("{0:?} is not a structure")]
    NotAStructure(UnitTypeId),
    /// A completed prerequisite structure is absent.
    #[error("missing prerequisite structure {0:?}")]
    MissingPrerequisite(UnitTypeId),
    /// Minerals are insufficient.
    #[error("insufficient minerals: need {required}, have {available}")]
    Minerals {
        /// Required minerals.
        required: u32,
        /// Current minerals.
        available: u32,
    },
    /// Gas is insufficient.
    #[error("insufficient gas: need {required}, have {available}")]
    Gas {
        /// Required gas.
        required: u32,
        /// Current gas.
        available: u32,
    },
    /// The footprint is outside the map.
    #[error("site lies outside the map")]
    OutOfBounds,
    /// Terrain or an existing structure refuses the footprint.
    #[error("site is blocked: {0:?}")]
    Blocked(PlacementFailureReason),
    /// A non-extractor footprint would bury a resource node.
    #[error("site covers a resource node")]
    CoversResource,
    /// An extractor must stand on a gas node and on no mineral node.
    #[error("an extractor needs a gas node under it")]
    ExtractorNeedsGas,
    /// Authoritative state is malformed.
    #[error(transparent)]
    Invariant(#[from] ProductionInvariant),
}

impl SimWorld {
    /// The footprint `structure_type` would have with its minimum corner at the given cell.
    fn site_of(
        &self,
        structure_type: UnitTypeId,
        cell_x: u32,
        cell_y: u32,
    ) -> Result<Footprint, BuildRejection> {
        let arch = self
            .catalog
            .get(structure_type)
            .map_err(|_| BuildRejection::UnknownStructure(structure_type))?;
        let ShapeKind::BuildingDome {
            footprint_w,
            footprint_h,
        } = arch.shape
        else {
            return Err(BuildRejection::NotAStructure(structure_type));
        };
        if u64::from(cell_x) + u64::from(footprint_w) > u64::from(self.map.width())
            || u64::from(cell_y) + u64::from(footprint_h) > u64::from(self.map.height())
        {
            return Err(BuildRejection::OutOfBounds);
        }
        Ok(Footprint {
            left: cell_x,
            bottom: cell_y,
            width: u32::from(footprint_w),
            height: u32::from(footprint_h),
        })
    }

    /// Everything `player` needs to start `structure_type` here, apart from a builder.
    fn check_build(
        &self,
        player: PlayerId,
        structure_type: UnitTypeId,
        cell_x: u32,
        cell_y: u32,
    ) -> Result<Footprint, BuildRejection> {
        let site = self.site_of(structure_type, cell_x, cell_y)?;
        let resources = self
            .player_resources
            .get(usize::from(player.0))
            .ok_or(ProductionInvariant::InvalidPlayer { player })?;
        let arch = self
            .catalog
            .get(structure_type)
            .map_err(|_| BuildRejection::UnknownStructure(structure_type))?;
        if let Some(missing) =
            self.missing_prerequisite(player, structure_type, &arch.requires_structures)?
        {
            return Err(BuildRejection::MissingPrerequisite(missing));
        }
        if resources.minerals < arch.mineral_cost {
            return Err(BuildRejection::Minerals {
                required: arch.mineral_cost,
                available: resources.minerals,
            });
        }
        if resources.gas < arch.gas_cost {
            return Err(BuildRejection::Gas {
                required: arch.gas_cost,
                available: resources.gas,
            });
        }
        #[expect(
            clippy::cast_possible_truncation,
            reason = "footprint sides were read from u8 fields in site_of"
        )]
        self.map
            .can_place_building(site.left, site.bottom, site.width as u8, site.height as u8)
            .map_err(|error| match error {
                sim_map::MapError::PlacementBlocked { reason, .. } => {
                    BuildRejection::Blocked(reason)
                }
                _ => BuildRejection::OutOfBounds,
            })?;
        let mut covers_gas = false;
        for node in self.map.resources() {
            if !site.contains_cell(node.cell) {
                continue;
            }
            if !arch.capabilities.is_gas_extractor {
                return Err(BuildRejection::CoversResource);
            }
            if node.kind != ResourceKind::Gas {
                return Err(BuildRejection::ExtractorNeedsGas);
            }
            covers_gas = true;
        }
        if arch.capabilities.is_gas_extractor && !covers_gas {
            return Err(BuildRejection::ExtractorNeedsGas);
        }
        Ok(site)
    }

    /// Admits a `Build` order for the nearest capable selected unit.
    ///
    /// A builder must have `can_construct` and share the structure's faction. Only that one
    /// unit takes the order; the rest of the selection is left alone. Nothing is paid yet.
    pub(crate) fn admit_build(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        order: &UnitOrder,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        let &UnitOrder::Build {
            structure_type,
            cell_x,
            cell_y,
        } = order
        else {
            return Err(GameCommandError::UnsupportedOrder);
        };
        self.validate_selection(player, recipients)?;
        let site = self.check_build(player, structure_type, cell_x, cell_y)?;
        let faction = self
            .catalog
            .get(structure_type)
            .map_err(|_| BuildRejection::UnknownStructure(structure_type))?
            .faction;
        let builder = self
            .nearest_recipient(recipients, site, |arch| {
                arch.capabilities.can_construct && arch.faction == faction
            })
            .ok_or(GameCommandError::NoCapableBuilder)?;
        self.commit_task_order(builder, order.clone(), queue)?;
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    }

    /// Admits a `Construct` order: the nearest capable selected unit resumes a foundation.
    pub(crate) fn admit_construct(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        foundation: EntityId,
        queue: bool,
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let target = self
            .entities
            .get(&foundation)
            .filter(|target| target.owner == player && target.is_alive())
            .filter(|target| target.construction.is_some())
            .ok_or(GameCommandError::NotAFoundation(foundation))?;
        let faction = self
            .catalog
            .get(target.unit_type_id)
            .map_err(|_| GameCommandError::NotAFoundation(foundation))?
            .faction;
        let site = self
            .map
            .structures()
            .get(&foundation)
            .map(Footprint::of_structure)
            .ok_or(GameCommandError::NotAFoundation(foundation))?;
        let builder = self
            .nearest_recipient(recipients, site, |arch| {
                arch.capabilities.can_construct && arch.faction == faction
            })
            .ok_or(GameCommandError::NoCapableBuilder)?;
        self.commit_task_order(builder, UnitOrder::Construct { foundation }, queue)?;
        Ok(GameCommandReceipt::Accepted { recipients: 1 })
    }

    /// Abandons the selected foundations, returning the documented share of their payment.
    ///
    /// All-or-nothing: if any selection is not this player's foundation, none is cancelled.
    pub(crate) fn cancel_foundations(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
    ) -> Result<GameCommandReceipt, GameCommandError> {
        self.validate_selection(player, recipients)?;
        let slot = usize::from(player.0);
        let mut economy = self.player_resources[slot];
        let mut refund = (0_u32, 0_u32);
        let share = |paid: u32| {
            let scaled = u64::from(paid) * u64::from(CONSTRUCTION_CANCEL_REFUND_PERCENT) / 100;
            u32::try_from(scaled).unwrap_or(u32::MAX)
        };
        let mut cancelled = Vec::with_capacity(recipients.len());
        for &id in recipients {
            if cancelled.contains(&id) {
                continue;
            }
            let record = self
                .entities
                .get(&id)
                .and_then(|entity| entity.construction)
                .ok_or(GameCommandError::NotAFoundation(id))?;
            let overflow = ProductionInvariant::Arithmetic {
                operation: "construction refund",
            };
            refund.0 = refund
                .0
                .checked_add(share(record.paid_minerals))
                .ok_or_else(|| overflow.clone())?;
            refund.1 = refund
                .1
                .checked_add(share(record.paid_gas))
                .ok_or_else(|| overflow.clone())?;
            cancelled.push(id);
        }
        let overflow = ProductionInvariant::Arithmetic {
            operation: "construction refund",
        };
        economy.minerals = economy
            .minerals
            .checked_add(refund.0)
            .ok_or_else(|| overflow.clone())?;
        economy.gas = economy.gas.checked_add(refund.1).ok_or(overflow)?;
        for id in cancelled {
            // The normal reap removes it this tick, releasing the footprint and any watcher.
            if let Some(entity) = self.entities.get_mut(&id) {
                entity.hp = SimScalar::ZERO;
            }
        }
        self.player_resources[slot] = economy;
        Ok(GameCommandReceipt::Refunded {
            minerals: refund.0,
            gas: refund.1,
        })
    }

    /// Prepares a foundation: a spawn with no provider credit and a construction record.
    fn prepare_foundation(
        &self,
        structure_type: UnitTypeId,
        owner: PlayerId,
        site: Footprint,
    ) -> Result<PreparedSpawn, ProductionInvariant> {
        let mut prepared = self.prepare_spawn(structure_type, owner, site.centre(), 0)?;
        let arch = self.catalog.get(structure_type).map_err(|_| {
            ProductionInvariant::MissingArchetype {
                entity: prepared.entity.id,
                unit_type: structure_type,
            }
        })?;
        prepared.economy.supply_provided_total = prepared
            .economy
            .supply_provided_total
            .checked_sub(arch.supply_provided)
            .ok_or(ProductionInvariant::Arithmetic {
                operation: "foundation provider credit",
            })?;
        prepared.economy.supply_max = prepared
            .economy
            .supply_provided_total
            .min(STANDARD_SUPPLY_CAP);
        // check_build proved both costs affordable against these same reserves.
        prepared.economy.minerals -= arch.mineral_cost;
        prepared.economy.gas -= arch.gas_cost;

        let entity = &mut prepared.entity;
        let start_hp = entity.max_hp / SimScalar::from_i32(FOUNDATION_HP_DIVISOR);
        let gained = entity.max_hp - start_hp;
        let ticks = arch.build_time_ticks;
        let divisor = i32::try_from(ticks.max(1)).unwrap_or(i32::MAX);
        let hp_per_tick = gained / SimScalar::from_i32(divisor);
        entity.hp = start_hp;
        entity.shields = SimScalar::ZERO;
        entity.is_completed = false;
        entity.captured_supply_provided = 0;
        entity.current_anim = AnimState::Constructing;
        entity.construction = Some(Construction {
            remaining_ticks: ticks,
            total_ticks: ticks,
            hp_per_tick,
            final_hp_residue: gained - hp_per_tick * SimScalar::from_i32(divisor),
            paid_minerals: arch.mineral_cost,
            paid_gas: arch.gas_cost,
            last_worked_tick: None,
        });
        Ok(prepared)
    }

    /// Lays the foundation and takes its payment in one transaction.
    fn place_foundation(
        &mut self,
        structure_type: UnitTypeId,
        owner: PlayerId,
        site: Footprint,
    ) -> Result<EntityId, BuildRejection> {
        let prepared = self.prepare_foundation(structure_type, owner, site)?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "footprint sides were read from u8 fields in site_of"
        )]
        self.map
            .place_structure(
                prepared.entity.id,
                site.left,
                site.bottom,
                site.width as u8,
                site.height as u8,
            )
            .map_err(|_| BuildRejection::OutOfBounds)?;
        let id = self.commit_spawn(prepared);
        self.pathfinder.recompute_clearance(&self.map);
        Ok(id)
    }

    /// Whether `entity` is a living ground body overlapping `site`.
    fn stands_on(&self, entity: &crate::Entity, site: Footprint) -> bool {
        if !entity.is_alive() || entity.is_loaded_in_transport() {
            return false;
        }
        self.catalog.get(entity.unit_type_id).is_ok_and(|arch| {
            crate::movement::MovementLayer::from_mover(arch.mover_type)
                == crate::movement::MovementLayer::Ground
                && !matches!(arch.shape, ShapeKind::BuildingDome { .. })
                && site
                    .gap(entity.position)
                    .is_some_and(|gap| gap < arch.collision_radius)
        })
    }

    /// One tick of a `Build` order at the front of `id`'s queue.
    pub(crate) fn step_build(
        &mut self,
        id: EntityId,
        structure_type: UnitTypeId,
        cell_x: u32,
        cell_y: u32,
    ) {
        let Some(owner) = self.entities.get(&id).map(|actor| actor.owner) else {
            return;
        };
        let site = match self.site_of(structure_type, cell_x, cell_y) {
            Ok(site) => site,
            Err(reason) => {
                self.end_task(id, Some(TaskEventKind::BuildAbandoned(reason)));
                return;
            }
        };
        if self
            .entities
            .get(&id)
            .is_some_and(|actor| self.stands_on(actor, site))
        {
            // The builder has wandered onto its own site: step off before laying anything.
            self.leave_footprint(id, site);
            return;
        }
        match self.approach_footprint(id, site) {
            crate::economy::Approach::Walking => return,
            crate::economy::Approach::Unreachable => {
                self.end_task(id, Some(TaskEventKind::Unreachable));
                return;
            }
            crate::economy::Approach::Arrived => {}
        }
        if let Err(reason) = self.check_build(owner, structure_type, cell_x, cell_y) {
            self.end_task(id, Some(TaskEventKind::BuildAbandoned(reason)));
            return;
        }
        if self
            .entities
            .values()
            .any(|body| self.stands_on(body, site))
        {
            let waited = self.entities.get_mut(&id).map_or(u16::MAX, |actor| {
                actor.task.wait_ticks = actor.task.wait_ticks.saturating_add(1);
                actor.task.wait_ticks
            });
            if waited > SITE_PATIENCE_TICKS {
                self.end_task(id, Some(TaskEventKind::SiteObstructed));
            }
            return;
        }
        match self.place_foundation(structure_type, owner, site) {
            Ok(foundation) => self.replace_task_order(id, UnitOrder::Construct { foundation }),
            Err(reason) => {
                if let BuildRejection::Invariant(source) = &reason {
                    self.report_production_failure(source.clone());
                }
                self.end_task(id, Some(TaskEventKind::BuildAbandoned(reason)));
            }
        }
    }

    /// One tick of a `Construct` order at the front of `id`'s queue.
    pub(crate) fn step_construct(&mut self, id: EntityId, foundation: EntityId) {
        let Some(owner) = self.entities.get(&id).map(|actor| actor.owner) else {
            return;
        };
        let workable = self
            .entities
            .get(&foundation)
            .filter(|target| target.owner == owner && target.is_alive());
        let Some(target) = workable else {
            self.end_task(id, Some(TaskEventKind::FoundationLost));
            return;
        };
        if target.construction.is_none() {
            self.end_task(id, None);
            return;
        }
        let Some(site) = self
            .map
            .structures()
            .get(&foundation)
            .map(Footprint::of_structure)
        else {
            self.end_task(id, Some(TaskEventKind::FoundationLost));
            return;
        };
        match self.approach_footprint(id, site) {
            crate::economy::Approach::Walking => {}
            crate::economy::Approach::Unreachable => {
                self.end_task(id, Some(TaskEventKind::Unreachable));
            }
            crate::economy::Approach::Arrived => {
                if let Some(actor) = self.entities.get_mut(&id) {
                    actor.current_anim = AnimState::Cast;
                }
                if self.work_on_foundation(foundation) {
                    self.end_task(id, Some(TaskEventKind::StructureCompleted { foundation }));
                }
            }
        }
    }

    /// Advances `foundation` by one worked tick at most once per tick; true when it completes.
    fn work_on_foundation(&mut self, foundation: EntityId) -> bool {
        let tick = Tick(self.current_tick);
        let Some(entity) = self.entities.get_mut(&foundation) else {
            return false;
        };
        let Some(record) = entity.construction.as_mut() else {
            return false;
        };
        if record.last_worked_tick == Some(tick) {
            return false;
        }
        record.last_worked_tick = Some(tick);
        record.remaining_ticks = record.remaining_ticks.saturating_sub(1);
        let finished = record.remaining_ticks == 0;
        let gain = if finished {
            record.hp_per_tick + record.final_hp_residue
        } else {
            record.hp_per_tick
        };
        entity.hp = (entity.hp + gain).min(entity.max_hp);
        if finished {
            self.finish_structure(foundation);
        }
        finished
    }

    /// Turns a worked-out foundation into a working building.
    fn finish_structure(&mut self, foundation: EntityId) {
        let Some(entity) = self.entities.get(&foundation) else {
            return;
        };
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            self.report_production_failure(ProductionInvariant::MissingArchetype {
                entity: foundation,
                unit_type: entity.unit_type_id,
            });
            return;
        };
        let (shields, provided) = (arch.max_shields, arch.supply_provided);
        let slot = usize::from(entity.owner.0);
        let mut economy = self.player_resources[slot];
        let Some(total) = economy.supply_provided_total.checked_add(provided) else {
            self.report_production_failure(ProductionInvariant::Arithmetic {
                operation: "completed provider total",
            });
            return;
        };
        economy.supply_provided_total = total;
        economy.supply_max = total.min(STANDARD_SUPPLY_CAP);
        self.player_resources[slot] = economy;
        if let Some(entity) = self.entities.get_mut(&foundation) {
            entity.construction = None;
            entity.is_completed = true;
            entity.captured_supply_provided = provided;
            entity.shields = shields;
            entity.current_anim = AnimState::Idle;
        }
    }

    /// Walks `id` off a site it is standing on.
    fn leave_footprint(&mut self, id: EntityId, site: Footprint) {
        let Some(actor) = self.entities.get(&id) else {
            return;
        };
        if actor.task.goal.is_some() {
            return;
        }
        match self.approach_point(actor.position, site) {
            Some(goal) if actor.task.legs < crate::economy::MAX_TASK_LEGS => {
                self.set_task_goal(id, goal);
            }
            _ => self.end_task(id, Some(TaskEventKind::Unreachable)),
        }
    }
}
