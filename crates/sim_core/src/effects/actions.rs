//! The leaf actions of the effect engine: spell damage with its responses (plan D-9), vital
//! changes and order cancellation (D-1).
//!
//! Spell damage is absorbed by a raised barrier first (B-41), then by shields, then by life;
//! armour never applies. A `Damage` of 0 changes nothing but raises its responses.

use crate::SimWorld;
use sim_types::{
    AttributeBonuses, DamageResponses, EffectContext, EntityId, SimAngle, SimScalar, SimVec2,
    VitalChange,
};

/// `change + change_fraction × current`, clamped to `0..=max`.
fn changed_vital(current: SimScalar, max: SimScalar, change: VitalChange) -> SimScalar {
    current
        .saturating_add(change.change)
        .saturating_add(change.change_fraction.saturating_mul(current))
        .clamp(SimScalar::ZERO, max.max(SimScalar::ZERO))
}

impl SimWorld {
    /// Deals spell damage of `amount` plus the bonus for each of the target's attributes, then
    /// raises `responses` against the context's caster and from its source point (D-9).
    pub(crate) fn apply_spell_damage(
        &mut self,
        unit: EntityId,
        amount: SimScalar,
        bonus: &AttributeBonuses,
        responses: DamageResponses,
        context: &EffectContext,
    ) {
        let Some(entity) = self.entities.get_mut(&unit) else {
            return;
        };
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            return;
        };
        let total = bonus
            .against(arch.attributes)
            .fold(amount, SimScalar::saturating_add);
        if total > SimScalar::ZERO {
            let dealt = entity
                .barrier
                .absorb(arch.capabilities.barrier.as_ref(), total);
            if dealt < total {
                self.presentation.record_barrier_hit(unit);
            }
            if dealt > SimScalar::ZERO {
                let shield = dealt.min(entity.shields);
                entity.apply_damage(shield, dealt - shield, SimAngle::ZERO);
            }
        }
        if responses.acquire || responses.flee {
            self.apply_spell_responses(unit, context.caster, context.source_point, responses);
        }
    }

    /// Changes life, shields and energy of `unit`, in that order, each clamped to its maximum.
    pub(crate) fn modify_vitals(&mut self, unit: EntityId, changes: [Option<VitalChange>; 3]) {
        let Some(entity) = self.entities.get_mut(&unit) else {
            return;
        };
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            return;
        };
        let [life, shields, energy] = changes;
        if let Some(change) = life {
            entity.hp = changed_vital(entity.hp, entity.max_hp, change);
        }
        if let Some(change) = shields {
            entity.shields = changed_vital(entity.shields, arch.max_shields, change);
        }
        if let Some(change) = energy {
            entity.energy = changed_vital(entity.energy, arch.max_energy, change);
        }
    }

    /// Clears the orders, hold, target, route and weapon swing of `unit` (`CEffectCancelOrder`).
    /// A mode channel in progress continues; with `force_unburrow`, a burrowed mode is left at
    /// once (D-8).
    pub(crate) fn cancel_orders_by_effect(&mut self, unit: EntityId, force_unburrow: bool) {
        let Some(entity) = self.entities.get_mut(&unit) else {
            return;
        };
        entity.cancel_orders_for_effect();
        if !force_unburrow {
            return;
        }
        let Ok(arch) = self.catalog.get(entity.unit_type_id) else {
            return;
        };
        let burrowed = entity
            .active_mode
            .as_deref()
            .and_then(|mode| arch.get_mode(mode))
            .is_some_and(|mode| mode.burrowed);
        if burrowed {
            entity.leave_mode(arch);
            entity.velocity = SimVec2::ZERO;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_vital_change_adds_points_and_a_fraction_then_clamps() {
        let ten = SimScalar::from_i32(10);
        let hundred = SimScalar::from_i32(100);
        let change = |change: i32, fraction: SimScalar| VitalChange {
            change: SimScalar::from_i32(change),
            change_fraction: fraction,
        };
        assert_eq!(
            changed_vital(ten, hundred, change(5, SimScalar::ZERO)),
            SimScalar::from_i32(15)
        );
        assert_eq!(
            changed_vital(ten, hundred, change(0, -SimScalar::ONE)),
            SimScalar::ZERO
        );
        assert_eq!(
            changed_vital(ten, hundred, change(0, SimScalar::HALF)),
            SimScalar::from_i32(15)
        );
        assert_eq!(
            changed_vital(ten, hundred, change(500, SimScalar::ZERO)),
            hundred
        );
        assert_eq!(
            changed_vital(ten, hundred, change(-50, SimScalar::ZERO)),
            SimScalar::ZERO
        );
    }
}
