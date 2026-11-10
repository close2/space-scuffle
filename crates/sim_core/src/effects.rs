//! # The effect-tree engine (plan `2026-11-10-spellcaster-mechanics`, Amendment 1)
//!
//! ## Responsibility
//! Owns the live state effect trees leave behind (statuses, persistents, missiles and
//! displacements, [`state`]) and the engine that evaluates the flattened trees of the spell
//! catalog ([`crate::catalog::SpellCatalog`]).
//!
//! ## How it fits
//! `SimWorld::effects` holds an [`EffectWorld`]; `Entity::displacement` holds a
//! [`Displacement`]. The checksum hashes both (D-13). Specs are catalog data, referred to by
//! [`sim_types::EffectId`] and [`sim_types::StatusId`].
//!
//! ## Invariants
//! - Every instance is fixed-point and iterated in id or `(EntityId, StatusId)` order (D-16).
//! - The engine never names a spell, unit or status (C5).

pub mod state;

pub use state::{
    ActiveMissile, ActivePersistent, ActiveStatus, Displacement, EffectWorld, PersistentAnchor,
};
