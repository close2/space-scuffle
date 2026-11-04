//! Pooled material slots for combat effects (remediation finding C-17).
//!
//! # Responsibility
//! This module owns the lifetime of every [`CombatFxMaterial`] asset. A material is a reusable
//! *slot*: [`CombatFxMaterialPool::acquire`] hands one to a new effect, and the effect's
//! [`CombatFxInstance`] hands it back when the effect's entity loses the component, whichever way
//! that happens (lifetime end, a scene teardown, the editor's playtest exit).
//!
//! # How it fits
//! The effect's shader data lives in its [`CombatFxInstance`] component, not in the asset. The
//! update systems of `vfx` write the component, and `vfx::gpu_upload` copies a changed component
//! into its slot's existing GPU uniform buffer during extraction. So once the pool has grown to the
//! peak number of live effects, combat raises no `AssetEvent` for this material at all, and Bevy
//! never creates a uniform buffer or a bind group for an effect again.
//!
//! # Invariants
//! - A slot is held by at most one live entity: it is in the free list or in exactly one entity's
//!   [`MeshMaterial3d`], never both. Every entity with a [`CombatFxInstance`] got its material from
//!   [`CombatFxMaterialPool::acquire`], and removes the component only together with the material
//!   (by despawning).
//! - The pool never removes an asset, so its strong handles keep every slot's prepared GPU material
//!   alive for the rest of the session.
//! - The asset's own `uniform` is only the slot's initial contents. The live state of an effect is
//!   its [`CombatFxInstance`]; no code reads the asset for it.

use super::{CombatFxMaterial, CombatFxUniformGpu};
use bevy::ecs::component::ComponentId;
use bevy::ecs::world::DeferredWorld;
use bevy::prelude::*;

/// The live shader data of one combat effect, which the render world copies to its slot.
///
/// Removing this component (normally by despawning the entity) returns the entity's material
/// slot to the [`CombatFxMaterialPool`].
#[derive(Component, Debug, Clone, Copy, PartialEq)]
#[component(on_remove = return_slot_to_pool)]
pub struct CombatFxInstance {
    /// Uniform the effect's material binding 0 must hold when the effect is drawn.
    pub uniform: CombatFxUniformGpu,
}

impl CombatFxInstance {
    /// Writes `age` (normalised lifetime progress, 0 to 1) into `instance` and marks it changed
    /// only when the value differs, so a paused frame uploads nothing.
    pub fn set_age(instance: &mut Mut<'_, Self>, age: f32) {
        let uniform = CombatFxUniformGpu {
            age,
            ..instance.uniform
        };
        instance.set_if_neq(Self { uniform });
    }
}

/// Reusable [`CombatFxMaterial`] slots, kept at the peak number of live combat effects.
// PERFORMANCE RATIONALE:
// Why: One material per effect meant an asset creation and removal per effect (17 per death), and
// writing each live effect's age through `Assets::get_mut` re-prepared its material every frame: a
// new uniform buffer and bind group per live effect per frame (finding C-17).
// What: Materials are slots handed out and returned here; the per-effect data lives in
// `CombatFxInstance`, and the render world writes it into the slot's existing buffer.
// Trade-off: Slots stay allocated at the session's peak count, and the asset's CPU copy is not the
// live state of an effect, so readers must use `CombatFxInstance`.
#[derive(Resource, Debug, Default)]
pub struct CombatFxMaterialPool {
    /// Slots no live effect holds. The most recently returned slot is reused first.
    free: Vec<Handle<CombatFxMaterial>>,
    /// Material assets this pool has created over the session.
    created: u64,
    /// Slots held by live effects.
    in_use: u64,
    /// Highest value [`Self::in_use`] has reached.
    peak_in_use: u64,
}

impl CombatFxMaterialPool {
    /// Hands out a slot for a new effect whose shader data starts as `initial`.
    ///
    /// Reuses a free slot when there is one. Otherwise adds a new material to `materials`,
    /// initialised with `initial`, so its first prepared buffer already holds the right data. A
    /// reused slot gets `initial` through the render-world upload of the effect's
    /// [`CombatFxInstance`], which is new and therefore changed.
    pub fn acquire(
        &mut self,
        materials: &mut Assets<CombatFxMaterial>,
        initial: CombatFxUniformGpu,
    ) -> Handle<CombatFxMaterial> {
        self.in_use += 1;
        self.peak_in_use = self.peak_in_use.max(self.in_use);
        if let Some(slot) = self.free.pop() {
            return slot;
        }
        self.created += 1;
        materials.add(CombatFxMaterial { uniform: initial })
    }

    /// Returns `slot` to the free list. Called by the removal hook of [`CombatFxInstance`].
    pub fn release(&mut self, slot: Handle<CombatFxMaterial>) {
        self.in_use = self.in_use.saturating_sub(1);
        self.free.push(slot);
    }

    /// The components that give a new effect its slot and its live shader data, `uniform`.
    pub fn effect(
        &mut self,
        materials: &mut Assets<CombatFxMaterial>,
        uniform: CombatFxUniformGpu,
    ) -> (CombatFxInstance, MeshMaterial3d<CombatFxMaterial>) {
        let slot = self.acquire(materials, uniform);
        (CombatFxInstance { uniform }, MeshMaterial3d(slot))
    }

    /// Material assets this pool has created over the session.
    #[must_use]
    pub fn created(&self) -> u64 {
        self.created
    }

    /// Slots currently held by live effects.
    #[must_use]
    pub fn in_use(&self) -> u64 {
        self.in_use
    }

    /// Highest number of slots live effects have held at once.
    #[must_use]
    pub fn peak_in_use(&self) -> u64 {
        self.peak_in_use
    }

    /// Slots waiting for a new effect.
    #[must_use]
    pub fn free_slots(&self) -> usize {
        self.free.len()
    }
}

/// Removal hook of [`CombatFxInstance`]: returns the entity's material slot to the pool.
///
/// On a despawn Bevy runs every removal hook before it removes any component, so the entity's
/// [`MeshMaterial3d`] is still readable here. Without a pool (an App that never added one) there is
/// nothing to return to.
fn return_slot_to_pool(mut world: DeferredWorld<'_>, entity: Entity, _: ComponentId) {
    let Some(slot) = world
        .get::<MeshMaterial3d<CombatFxMaterial>>(entity)
        .map(|material| material.0.clone())
    else {
        return;
    };
    if let Some(mut pool) = world.get_resource_mut::<CombatFxMaterialPool>() {
        pool.release(slot);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed test is a panic; each message states the invariant it relies on"
)]
mod tests {
    use super::super::{update_beams_system, BeamVisualComponent};
    use super::*;
    use bevy::time::TimeUpdateStrategy;
    use std::collections::BTreeSet;
    use std::time::Duration;

    /// Frame length of the scripted apps.
    const FRAME: Duration = Duration::from_millis(50);

    /// Lifetimes, in seconds, the scripted beams draw from: one to eight frames.
    const LIFETIMES: [f32; 4] = [0.05, 0.1, 0.2, 0.4];

    fn uniform(seed: f32) -> CombatFxUniformGpu {
        CombatFxUniformGpu {
            seed,
            ..CombatFxUniformGpu::default()
        }
    }

    /// A world with the material store and the pool, as the client registers them.
    fn world() -> World {
        let mut world = World::new();
        world.init_resource::<Assets<CombatFxMaterial>>();
        world.init_resource::<CombatFxMaterialPool>();
        world
    }

    /// Spawns one effect entity through the pool, as the production spawner does.
    fn spawn_effect(world: &mut World, seed: f32) -> Entity {
        let components = world.resource_scope(|world, mut pool: Mut<'_, CombatFxMaterialPool>| {
            let mut materials = world.resource_mut::<Assets<CombatFxMaterial>>();
            pool.effect(&mut materials, uniform(seed))
        });
        world.spawn(components).id()
    }

    fn slot(world: &World, entity: Entity) -> AssetId<CombatFxMaterial> {
        world
            .get::<MeshMaterial3d<CombatFxMaterial>>(entity)
            .expect("invariant: spawn_effect inserts the material")
            .0
            .id()
    }

    #[test]
    fn acquire_reuses_a_released_slot() {
        let mut world = world();
        let first = spawn_effect(&mut world, 0.1);
        let used = slot(&world, first);
        world.despawn(first);
        let second = spawn_effect(&mut world, 0.2);
        assert_eq!(slot(&world, second), used, "the released slot is reused");
        let pool = world.resource::<CombatFxMaterialPool>();
        assert_eq!(pool.created(), 1);
        assert_eq!(pool.in_use(), 1);
        assert_eq!(world.resource::<Assets<CombatFxMaterial>>().len(), 1);
    }

    #[test]
    fn a_new_slot_starts_with_the_effect_data() {
        let mut world = world();
        let entity = spawn_effect(&mut world, 0.75);
        let id = slot(&world, entity);
        let material = world
            .resource::<Assets<CombatFxMaterial>>()
            .get(id)
            .expect("invariant: the pool added this material");
        assert_eq!(material.uniform, uniform(0.75));
    }

    #[test]
    fn set_age_marks_a_change_only_when_the_age_differs() {
        let mut world = world();
        let entity = spawn_effect(&mut world, 0.0);
        world.clear_trackers();
        let mut instance = world
            .get_mut::<CombatFxInstance>(entity)
            .expect("invariant: spawned with an instance");
        CombatFxInstance::set_age(&mut instance, 0.0);
        assert!(!instance.is_changed(), "an equal age is not a change");
        CombatFxInstance::set_age(&mut instance, 0.5);
        assert_eq!(instance.uniform.age.to_bits(), 0.5_f32.to_bits());
        assert!(instance.is_changed());
    }

    #[test]
    fn the_peak_counts_simultaneous_effects() {
        let mut world = world();
        let effects: Vec<Entity> = (0..5_u8)
            .map(|i| spawn_effect(&mut world, f32::from(i)))
            .collect();
        for entity in effects {
            world.despawn(entity);
        }
        let _ = spawn_effect(&mut world, 9.0);
        let pool = world.resource::<CombatFxMaterialPool>();
        assert_eq!(
            (pool.created(), pool.in_use(), pool.peak_in_use()),
            (5, 1, 5)
        );
        assert_eq!(pool.free_slots(), 4);
    }

    /// An App that ages beams with the production update system, at [`FRAME`] per update.
    fn effect_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<Assets<CombatFxMaterial>>()
            .init_resource::<CombatFxMaterialPool>()
            .insert_resource(TimeUpdateStrategy::ManualDuration(FRAME))
            .add_systems(Update, update_beams_system);
        app
    }

    /// Spawns a beam effect through the pool that lives `lifetime` seconds.
    fn spawn_beam(world: &mut World, lifetime: f32) -> Entity {
        let effect = spawn_effect(world, lifetime);
        world
            .entity_mut(effect)
            .insert((
                Transform::default(),
                BeamVisualComponent {
                    from: Vec3::ZERO,
                    to: Vec3::X,
                    is_lightning: false,
                    lifetime: Timer::from_seconds(lifetime, TimerMode::Once),
                },
            ))
            .id()
    }

    /// The slot of every live effect, in query order.
    fn live_slots(world: &mut World) -> Vec<AssetId<CombatFxMaterial>> {
        world
            .query_filtered::<&MeshMaterial3d<CombatFxMaterial>, With<CombatFxInstance>>()
            .iter(world)
            .map(|material| material.0.id())
            .collect()
    }

    /// Asserts that no slot is held twice, by two effects or by an effect and the free list.
    fn assert_slots_exclusive(world: &mut World) {
        let live = live_slots(world);
        let distinct: BTreeSet<_> = live.iter().copied().collect();
        assert_eq!(distinct.len(), live.len(), "two live effects share a slot");
        let pool = world.resource::<CombatFxMaterialPool>();
        assert_eq!(pool.in_use(), live.len() as u64, "in-use count");
        assert!(
            pool.free.iter().all(|slot| !distinct.contains(&slot.id())),
            "a free slot is held by a live effect"
        );
    }

    #[test]
    fn a_slot_is_never_held_by_two_live_effects() {
        let mut app = effect_app();
        // A fixed xorshift sequence, so the script is the same on every run.
        let mut state: usize = 0x2545_f491_4f6c_dd1d;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut spawned = 0;
        let mut despawned_early = 0;
        while spawned < 500 {
            let world = app.world_mut();
            for _ in 0..next() % 6 {
                spawn_beam(world, LIFETIMES[next() % LIFETIMES.len()]);
                spawned += 1;
            }
            if next() % 3 == 0 {
                let live: Vec<Entity> = world
                    .query_filtered::<Entity, With<CombatFxInstance>>()
                    .iter(world)
                    .collect();
                if let Some(entity) = live.get(next() % live.len().max(1)) {
                    world.entity_mut(*entity).despawn_recursive();
                    despawned_early += 1;
                }
            }
            assert_slots_exclusive(world);
            app.update();
            assert_slots_exclusive(app.world_mut());
        }
        let pool = app.world().resource::<CombatFxMaterialPool>();
        assert!(despawned_early > 0, "the script despawns effects early");
        assert!(
            pool.created() < 500 && pool.created() == pool.peak_in_use(),
            "the pool grows only to the peak of live effects: {pool:?}"
        );
    }

    #[test]
    fn despawning_by_any_path_returns_the_slot() {
        let mut app = effect_app();
        let returned = |app: &App| {
            let pool = app.world().resource::<CombatFxMaterialPool>();
            (pool.in_use(), pool.free_slots(), pool.created())
        };

        // Lifetime end, in the production update system.
        spawn_beam(app.world_mut(), LIFETIMES[0]);
        for _ in 0..3 {
            app.update();
        }
        assert!(live_slots(app.world_mut()).is_empty(), "the beam expired");
        assert_eq!(returned(&app), (0, 1, 1), "lifetime end");

        // A scene teardown: `despawn_recursive` through commands, on an effect parented to a root.
        let beam = spawn_beam(app.world_mut(), 10.0);
        let root = app.world_mut().spawn_empty().add_child(beam).id();
        app.world_mut().commands().entity(root).despawn_recursive();
        app.world_mut().flush();
        assert_eq!(returned(&app), (0, 1, 1), "recursive teardown");

        // The editor's playtest exit: every effect material, despawned directly.
        let beam = spawn_beam(app.world_mut(), 10.0);
        app.world_mut().commands().entity(beam).despawn_recursive();
        app.world_mut().flush();
        assert_eq!(returned(&app), (0, 1, 1), "playtest teardown");

        // App teardown with a live effect: the world drops its entities and the pool together.
        spawn_beam(app.world_mut(), 10.0);
        assert_eq!(returned(&app), (1, 0, 1));
        drop(app);
    }
}
