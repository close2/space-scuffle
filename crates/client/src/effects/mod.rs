//! The effect system: data-defined, layered effects drawn from the simulation's facts (plan
//! `docs/plans/2026-11-09-effect-visualization.md`, WP4).
//!
//! # Responsibility
//! Owns the effect vocabulary ([`catalog`]), its pure evaluation ([`curve`], [`binding`],
//! [`placement`]), the live effect instances and their budget and level of detail, and the
//! instanced renderer of the `Sprite`, `Ring`, `Decal`, `Light` and CPU `Particles` blocks.
//!
//! # How it fits
//! The simulation publishes facts (`CombatEventVisual::SplashVisual`, every weapon event's
//! `EffectSource`); the battle pipeline admits each event by fog where it lands and sends it
//! as a [`crate::vfx::CombatVisualEvent`]. This module looks the source unit's choice up in
//! [`catalog::UnitEffectTables`] and spawns the effect. Nothing here feeds back into the
//! simulation. The older `vfx` module keeps drawing tracers, beams and the shatter until the
//! plan's WP11 moves them into effect definitions.
//!
//! # Invariants
//! - No Rust names a unit, weapon, mode or effect (charter C5); every look is data read at run
//!   time (C6).
//! - Effect time is `Time<Virtual>`, never the wall clock; randomness is a hash of the spawn
//!   sequence and layer index, never RNG state.
//! - Pools are sized from the catalog's budget at load and never grow during a battle.

pub mod binding;
pub mod budget;
pub mod catalog;
pub mod curve;
pub mod instances;
pub mod particles;
pub mod placement;
pub mod pool;
pub mod render;
pub mod runtime;
pub mod systems;

use bevy::prelude::*;

/// Registers the effect system: loading at startup, event wiring in the battle's present
/// stage, the per-frame build after transforms propagate, the light pool and the renderer.
pub struct EffectsPlugin;

impl Plugin for EffectsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<render::EffectCatalogVersion>()
            .add_systems(Startup, systems::load_effect_catalog)
            .add_systems(
                Update,
                systems::spawn_effects_from_events
                    .after(crate::battle_pipeline::driver::publish_combat_events)
                    .in_set(crate::battle_pipeline::BattleSet::Present),
            )
            .add_systems(
                PostUpdate,
                (systems::build_effect_frame, systems::apply_effect_lights)
                    .chain()
                    .after(TransformSystem::TransformPropagate),
            )
            .add_plugins(render::EffectRenderPlugin);
    }
}
