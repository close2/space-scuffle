//! Research jobs: admission and completion over the shared production queue.
//!
//! An upgrade is one more job in a structure's FIFO queue. It is admitted by the same
//! recipient selection, payment and commit as training, and cancelled by the same refund.
//! What differs is only what it needs and delivers: no supply, and an upgrade instead of a
//! unit.
//!
//! # Invariants
//! - Research is idempotent: an upgrade a player owns, or has in any queue, is refused, so
//!   no effect can apply twice.
//! - A tiered upgrade needs the tier below it to have finished.
//! - A job whose research cannot be applied is removed and refunded with a diagnostic, so a
//!   malformed upgrade cannot hold a queue forever or swallow the payment.

use super::{
    ProductionAdmissionError, ProductionInvariant, ProductionItem, ProductionJob, ProductionReceipt,
};
use crate::upgrades::{UpgradeDef, UpgradeId};
use crate::{PlayerResources, SimWorld};
use sim_types::{EntityId, PlayerId, PlayerUpgradeState, UpgradeEffect};

/// Whether `def` is a tier whose preceding tier `state` has not finished.
fn tier_locked(def: &UpgradeDef, state: &PlayerUpgradeState) -> bool {
    def.effects.iter().any(|effect| {
        let (held, level) = match effect {
            UpgradeEffect::WeaponDamageLevel {
                weapon_class,
                level,
            } => (state.weapon_levels.get(weapon_class).copied(), *level),
            UpgradeEffect::ArmorLevel { armor_class, level } => {
                (state.armor_levels.get(armor_class).copied(), *level)
            }
            UpgradeEffect::ShieldLevel { level } => (Some(state.shield_level), *level),
            UpgradeEffect::ModifyUnitStat { .. } | UpgradeEffect::UnlockAbility { .. } => {
                return false
            }
        };
        held.unwrap_or(0).saturating_add(1) < level
    })
}

impl SimWorld {
    /// Admits one research job to the first completed selected structure that researches it.
    ///
    /// All selections must exist, be alive and owned. Minerals and gas are taken now; research
    /// reserves no supply. The upgrade applies when the countdown ends.
    ///
    /// # Errors
    /// Rejections preserve authoritative state. Corruption additionally emits a fatal diagnostic.
    pub fn try_queue_research(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        upgrade: UpgradeId,
    ) -> Result<ProductionReceipt, ProductionAdmissionError> {
        if let Err(source) = self.validate_transport_links() {
            let error = ProductionInvariant::Transport(source);
            self.report_production_failure(error.clone());
            return Err(error.into());
        }
        self.queue_research_prevalidated(player, recipients, upgrade)
    }

    // Public wrapper or normal tick validated relations; admission forbids passenger work.
    pub(crate) fn queue_research_prevalidated(
        &mut self,
        player: PlayerId,
        recipients: &[EntityId],
        upgrade: UpgradeId,
    ) -> Result<ProductionReceipt, ProductionAdmissionError> {
        let admitted = self.prepare_research(player, recipients, upgrade);
        self.commit_job(player, admitted)
    }

    fn prepare_research(
        &self,
        player: PlayerId,
        recipients: &[EntityId],
        upgrade: UpgradeId,
    ) -> Result<(ProductionReceipt, PlayerResources), ProductionAdmissionError> {
        let Some(resources) = self.player_resources.get(usize::from(player.0)) else {
            return Err(ProductionAdmissionError::InvalidPlayer(player));
        };
        let def = self
            .upgrade_catalog
            .get_by_id(upgrade)
            .ok_or(ProductionAdmissionError::UnknownUpgrade(upgrade))?;
        let producer = self
            .select_producer(player, recipients, |source| {
                Ok(source.researches_upgrades.contains(&def.slug))
            })?
            .ok_or(ProductionAdmissionError::NoEligibleResearcher(upgrade))?;
        let state = &self.player_upgrades[usize::from(player.0)];
        if state.completed_upgrade_slugs.contains(&def.slug) {
            return Err(ProductionAdmissionError::UpgradeAlreadyResearched(upgrade));
        }
        let queued = self
            .entities
            .values()
            .filter(|entity| entity.owner == player)
            .flat_map(|entity| &entity.production_queue)
            .any(|job| job.item == ProductionItem::Research(upgrade));
        if queued {
            return Err(ProductionAdmissionError::UpgradeAlreadyQueued(upgrade));
        }
        if tier_locked(def, state) {
            return Err(ProductionAdmissionError::UpgradeTierLocked(upgrade));
        }
        self.validate_supply_ledger()?;
        let source = self
            .entities
            .get(&producer)
            .ok_or(ProductionInvariant::EntityState {
                entity: producer,
                reason: "selected researcher disappeared during preparation",
            })?;
        let job = ProductionJob {
            item: ProductionItem::Research(upgrade),
            remaining_ticks: def.research_ticks,
            total_ticks: def.research_ticks,
            reserved_supply: 0,
            paid_minerals: def.mineral_cost,
            paid_gas: def.gas_cost,
        };
        super::lifecycle::validate_job_source(
            &self.catalog,
            &self.upgrade_catalog,
            &self.map,
            source,
            &job,
        )?;
        if let Some(missing) =
            self.missing_prerequisite(player, source.unit_type_id, &def.requires_structures)?
        {
            return Err(ProductionAdmissionError::MissingPrerequisite(missing));
        }
        let economy = Self::charge_job(resources, &job)?;
        Ok((ProductionReceipt { producer, job }, economy))
    }

    /// Applies a finished research job, or refunds it with a diagnostic if it cannot apply.
    pub(super) fn complete_research(
        &mut self,
        producer: EntityId,
        owner: PlayerId,
        upgrade: UpgradeId,
        job: &ProductionJob,
    ) {
        let slug = self
            .upgrade_catalog
            .get_by_id(upgrade)
            .map(|def| def.slug.clone());
        let applied = slug.is_some_and(|slug| self.research_upgrade(owner, &slug).is_ok());
        if applied {
            if let Some(entity) = self.entities.get_mut(&producer) {
                if entity.production_queue.front() == Some(job) {
                    entity.production_queue.pop_front();
                }
            }
            return;
        }
        self.report_production_failure(ProductionInvariant::EntityState {
            entity: producer,
            reason: "finished research could not be applied and was refunded",
        });
        if let Err(source) = self.cancel_production_job(producer, 0) {
            self.report_production_failure(source);
        }
    }
}
