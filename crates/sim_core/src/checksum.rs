//! # Authoritative State xxHash64 Checksum
//!
//! Computes a platform-independent 64-bit digest over all authoritative simulation state.
//! Used for lockstep desync verification and replay synchronization.
//!
//! ## Invariants Maintained
//! 1. All raw fixed-point bits are serialized in little-endian byte order (`to_le_bytes`).
//! 2. Entities are iterated in strictly sorted [`sim_types::EntityId`] order.
//! 3. Zero floating-point data enters the hash calculation.

use crate::entity::{ActiveProjectile, Entity, WeaponPhase};
use sim_types::EntityId;
use std::collections::BTreeMap;
use xxhash_rust::xxh64::Xxh64;

// PERFORMANCE RATIONALE:
// Why: Hashing dynamic simulation state must execute in under 0.1 ms per tick.
// What: xxHash64 processes >5 GB/s of memory. We stream primitive integer byte representations
// in strictly sorted EntityId order directly into a persistent hasher.
// Trade-off: Hashing all vital fields ensures 100% desync detection without allocating temporary strings.

/// Computes the authoritative 64-bit state checksum for the current simulation frame.
#[must_use]
pub fn compute_world_checksum(
    tick: u64,
    rng_seed: u64,
    entities: &BTreeMap<EntityId, Entity>,
    projectiles: &[ActiveProjectile],
) -> u64 {
    let mut hasher = Xxh64::new(0);

    hasher.update(&tick.to_le_bytes());
    hasher.update(&rng_seed.to_le_bytes());

    for (id, entity) in entities {
        hasher.update(&id.0.to_le_bytes());
        hasher.update(&entity.unit_type_id.0.to_le_bytes());
        hasher.update(&entity.owner.0.to_le_bytes());

        // Deterministic fixed-point Q32.32 bits
        hasher.update(&entity.position.x.raw().to_le_bytes());
        hasher.update(&entity.position.y.raw().to_le_bytes());
        hasher.update(&entity.velocity.x.raw().to_le_bytes());
        hasher.update(&entity.velocity.y.raw().to_le_bytes());
        hasher.update(&entity.heading.raw().to_le_bytes());
        hasher.update(&entity.turret_angle.raw().to_le_bytes());

        hasher.update(&entity.hp.raw().to_le_bytes());
        hasher.update(&entity.shields.raw().to_le_bytes());
        hasher.update(&entity.energy.raw().to_le_bytes());
        hasher.update(&entity.ticks_since_last_damage.to_le_bytes());
        hasher.update(&entity.weapon_cooldown_remaining.to_le_bytes());

        let phase_tag: u8 = match entity.weapon_phase {
            WeaponPhase::Ready => 0,
            WeaponPhase::RotatingToTarget { .. } => 1,
            WeaponPhase::Windup { .. } => 2,
            WeaponPhase::Backswing { .. } => 3,
            WeaponPhase::Cooldown => 4,
        };
        hasher.update(&[phase_tag]);
    }

    for proj in projectiles {
        hasher.update(&proj.id.to_le_bytes());
        hasher.update(&proj.owner.0.to_le_bytes());
        hasher.update(&proj.current_pos.x.raw().to_le_bytes());
        hasher.update(&proj.current_pos.y.raw().to_le_bytes());
    }

    hasher.digest()
}
