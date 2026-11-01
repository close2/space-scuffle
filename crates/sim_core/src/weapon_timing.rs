//! Bounded deterministic attack-rate and weapon-interval arithmetic.
//!
//! Positive rates use ordered Q32.32 multiplication, saturating only outside its
//! positive representable domain. Periods are resolved in wide integer arithmetic
//! and clamped once to the existing one-tick through `u16::MAX` timing range.

use crate::entity::ActiveBuff;
use sim_types::{EntityId, SimScalar, Tick};
use thiserror::Error;

/// An invalid nonpositive Q32.32 attack-rate input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("attack rate must be positive, received Q32.32 raw value {raw}")]
pub struct InvalidAttackRate {
    /// Invalid signed Q32.32 representation.
    pub raw: i64,
}

/// A strictly positive, representable Q32.32 attack-rate multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttackRate(SimScalar);

impl AttackRate {
    /// Identity rate, leaving the upgraded interval unchanged before clamping.
    pub const ONE: Self = Self(SimScalar::ONE);

    /// Validates a rate without replacing invalid values with a default.
    ///
    /// # Errors
    /// Returns [`InvalidAttackRate`] for zero or negative rates.
    pub fn new(value: SimScalar) -> Result<Self, InvalidAttackRate> {
        if value <= SimScalar::ZERO {
            Err(InvalidAttackRate { raw: value.raw() })
        } else {
            Ok(Self(value))
        }
    }

    /// Returns the positive Q32.32 multiplier.
    #[must_use]
    pub const fn value(self) -> SimScalar {
        self.0
    }
}

/// Invalid direct mutation of a buff's strictly positive attack-rate invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("buff index {buff_index} (ID {buff_id}): {source}")]
pub struct InvalidBuffAttackRate {
    /// Position in the entity's deterministic buff vector.
    pub buff_index: usize,
    /// Gameplay buff identifier.
    pub buff_id: u8,
    /// Invalid numeric rate evidence.
    #[source]
    pub source: InvalidAttackRate,
}

/// Explicit diagnostic output for an invalid state encountered during a tick.
///
/// Diagnostics do not affect simulation decisions; the malformed source state is
/// authoritative and already checksummed. Consumers must report these failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SimulationDiagnostic {
    /// Invalid carrier identity or passenger lifecycle suppressed an operation.
    #[error("simulation tick {tick:?}: {source}")]
    TransportInvariant {
        /// Tick at which the malformed relation was observed.
        tick: Tick,
        /// Exact invalid relation evidence.
        source: crate::transport::TransportInvariant,
    },
    /// An invalid movement definition or live state suppressed displacement.
    #[error("simulation tick {tick:?}: {source}")]
    MovementInvariant {
        /// Tick at which the state was observed.
        tick: Tick,
        /// Exact movement invariant failure.
        source: crate::movement::MovementInvariant,
    },
    /// A production/lifecycle operation was suppressed without partial commit.
    #[error("simulation tick {tick:?}: {source}")]
    ProductionInvariant {
        /// Tick at which the invalid state was observed.
        tick: Tick,
        /// Exact transaction invariant that failed.
        source: crate::production::ProductionInvariant,
    },
    /// An invalid buff prevented this entity's weapon update during the tick.
    #[error("simulation tick {tick:?}, entity {entity:?}: {source}")]
    InvalidBuffAttackRate {
        /// Tick in which the invalid state was observed.
        tick: Tick,
        /// Entity whose attack was suppressed.
        entity: EntityId,
        /// Exact invalid buff and value.
        #[source]
        source: InvalidBuffAttackRate,
    },
}

/// Folds buff rates in stored order using positive Q32.32 arithmetic.
///
/// Intermediate positive underflow saturates to raw 1; overflow saturates to
/// `i64::MAX`. Quantization and saturation make this fold non-associative, so a
/// later reciprocal cannot recover precision lost at an earlier step.
///
/// # Errors
/// Returns the first nonpositive rate with its vector index and buff ID.
pub fn combined_attack_rate(buffs: &[ActiveBuff]) -> Result<AttackRate, InvalidBuffAttackRate> {
    let mut raw = i128::from(SimScalar::ONE.raw());
    for (buff_index, buff) in buffs.iter().enumerate() {
        let rate = AttackRate::new(buff.attack_rate_multiplier).map_err(|source| {
            InvalidBuffAttackRate {
                buff_index,
                buff_id: buff.buff_id,
                source,
            }
        })?;
        let product = raw * i128::from(rate.value().raw());
        raw = (product >> 32).clamp(1, i128::from(i64::MAX));
    }
    // The preceding clamp bounds every intermediate; the initial identity also
    // fits i64, so this conversion is lossless.
    let raw = i64::try_from(raw).unwrap_or(i64::MAX);
    Ok(AttackRate(SimScalar::from_raw(raw)))
}

/// Resolves an interval in 25 Hz ticks with a single final clamp to `1..=u16::MAX`.
///
/// Adds signed upgrade ticks before rate scaling. Positive division rounds down;
/// nonpositive results become one tick, and long results saturate at the existing
/// timer's maximum. Wide integer intermediates preserve the complete numerator.
#[must_use]
pub fn resolve_attack_period(base_ticks: u16, delta_ticks: i32, rate: AttackRate) -> u16 {
    let numerator = (i128::from(base_ticks) + i128::from(delta_ticks)) << 32;
    let scaled = numerator / i128::from(rate.value().raw());
    let bounded = scaled.clamp(1, i128::from(u16::MAX));
    // The immediately preceding clamp proves this conversion cannot fail.
    u16::try_from(bounded).unwrap_or(u16::MAX)
}
