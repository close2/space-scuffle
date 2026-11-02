//! Production movement wakes: bounded path histories, reusable ribbons, and shared lighting.
//!
//! The plugin observes rendered bubbles only. It owns presentation time and pooled resources,
//! never changes authoritative simulation, and reports invalid input or budget exhaustion.

pub(crate) mod benchmark;
pub mod capture;
pub use benchmark::MistCpuTiming;
mod callback_gate;
mod completion;
pub mod emission;
pub mod gpu;
#[cfg(test)]
mod gpu_oracle_tests;
mod light_probe;
mod lighting;
#[cfg(test)]
mod lighting_gpu_tests;
pub mod material;
pub(crate) mod packet_witness;
pub(crate) mod scene_light_trace;
mod view_light_capture;
pub use lighting::MistLightError;
mod output_evidence;
#[cfg(test)]
mod output_tests;
pub mod reduction;
pub mod reduction_admission;
pub mod ribbon;
mod shell_metrics;
mod upload;

pub use material::{MistWakeMaterial, MistWakeUniformGpu};

use crate::{BubbleInstance, LabPreviewBubble, LabState, ResolvedAssetsRoot};
use bevy::prelude::*;
use bevy::render::primitives::Aabb;
use bevy::render::renderer::RenderDevice;
use bevy::transform::TransformSystem;
use emission::EmitterHistory;
use serde::Deserialize;
use sim_types::ShapeKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

/// Typed failures for style loading, bounded resource ownership, and presentation input.
#[derive(Debug, Error)]
pub enum MistError {
    /// Authored directional-light data cannot safely reach the view-bound shader.
    #[error(transparent)]
    Light(#[from] MistLightError),
    /// GPU capacity, publication or mapping failed explicitly.
    #[error(transparent)]
    Gpu(#[from] gpu::MistGpuError),
    /// Configuration could not be read from the exact runtime root.
    #[error("cannot read mist style {path:?}: {source}")]
    Io {
        /// Style file path.
        path: PathBuf,
        /// Original filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// RON configuration has invalid syntax.
    #[error("invalid mist style: {0}")]
    Ron(#[from] ron::error::SpannedError),
    /// Configuration violates a documented budget or physical-units constraint.
    #[error("invalid mist style: {0}")]
    InvalidStyle(&'static str),
    /// Non-finite motion or invalid radius/time was supplied by a visual entity.
    #[error(
        "mist motion must have finite position, positive radius, and nonnegative elapsed seconds"
    )]
    InvalidMotion,
    /// All configured emitter slots are occupied.
    #[error("mist emitter budget exhausted; increase the validated style capacity")]
    CapacityExhausted,
    /// A pooled mesh attribute no longer matches the fixed geometry contract.
    #[error("mist mesh attributes or indices violate the pooled geometry contract")]
    GeometryContract,
}

/// Unit-agnostic art controls and bounded pool capacity, loaded from `catalog/mist.ron`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MistStyle {
    /// Maximum simultaneous moving/fading bubble histories.
    pub max_emitters: usize,
    /// Ring-buffer samples retained per emitter.
    pub samples_per_emitter: usize,
    /// Persistent mapped transfer packets; bounded independently of emitter ownership.
    #[serde(default = "default_staging_buffers")]
    pub gpu_staging_buffers: usize,
    /// Maximum wall seconds of unavailable staging before an explicit rendering failure.
    #[serde(default = "default_stall_timeout")]
    pub gpu_stall_timeout_seconds: f32,
    /// Immutable reference illuminance in lux for dimensionless view-light response.
    #[serde(default = "lighting::default_reference_lux")]
    pub reference_illuminance_lux: f32,
    /// Maximum sample lifetime in presentation seconds.
    pub lifetime_seconds: f32,
    /// Spatial spacing as a fraction of rendered bubble radius.
    pub spacing_radii: f32,
    /// Maximum temporal spacing while moving, in seconds.
    pub max_sample_seconds: f32,
    /// Maximum continuous movement in one observed frame, in rendered radii.
    pub continuity_radii: f32,
    /// Broad strand half-width as a fraction of rendered radius.
    pub width_radii: f32,
    /// Global extinction/opacity in [0, 1].
    pub opacity: f32,
    /// Linear RGB body tint.
    pub cool_color: [f32; 3],
    /// Linear RGB iridescent ridge tint.
    pub warm_color: [f32; 3],
    /// Linear RGB overclock accent.
    pub boost_color: [f32; 3],
    /// Soft opaque-scene intersection distance, in world units.
    pub soft_intersection_units: f32,
}

const fn default_staging_buffers() -> usize {
    3
}
const fn default_stall_timeout() -> f32 {
    2.0
}

impl Default for MistStyle {
    fn default() -> Self {
        Self {
            max_emitters: 512,
            samples_per_emitter: 48,
            gpu_staging_buffers: default_staging_buffers(),
            gpu_stall_timeout_seconds: default_stall_timeout(),
            reference_illuminance_lux: lighting::default_reference_lux(),
            lifetime_seconds: 1.35,
            spacing_radii: 0.13,
            max_sample_seconds: 0.08,
            continuity_radii: 8.0,
            width_radii: 0.62,
            opacity: 0.60,
            cool_color: [0.24, 0.56, 0.72],
            warm_color: [0.82, 0.64, 0.9],
            boost_color: [1.0, 0.69, 0.32],
            soft_intersection_units: 0.4,
        }
    }
}

impl MistStyle {
    /// Loads and validates an explicit runtime configuration file.
    ///
    /// # Errors
    /// Returns [`MistError`] for I/O, syntax, or invalid style values.
    pub fn load(path: &Path) -> Result<Self, MistError> {
        let text = std::fs::read_to_string(path).map_err(|source| MistError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let style: Self = ron::from_str(&text)?;
        style.validate()?;
        Ok(style)
    }

    /// Validates finite art controls and a bounded, representable geometry budget.
    ///
    /// # Errors
    /// Returns [`MistError::InvalidStyle`] if any constraint is violated.
    pub fn validate(&self) -> Result<(), MistError> {
        lighting::validate_reference(self.reference_illuminance_lux)?;
        if !(1..=4096).contains(&self.max_emitters)
            || !(4..=128).contains(&self.samples_per_emitter)
        {
            return Err(MistError::InvalidStyle(
                "capacity requires 1..4096 emitters and 4..128 samples",
            ));
        }
        if !(2..=8).contains(&self.gpu_staging_buffers)
            || !self.gpu_stall_timeout_seconds.is_finite()
            || !(0.1..=30.0).contains(&self.gpu_stall_timeout_seconds)
        {
            return Err(MistError::InvalidStyle(
                "GPU staging requires 2..8 packets and 0.1..30 wall seconds",
            ));
        }
        if [
            self.lifetime_seconds,
            self.spacing_radii,
            self.max_sample_seconds,
            self.continuity_radii,
            self.width_radii,
            self.soft_intersection_units,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0)
        {
            return Err(MistError::InvalidStyle(
                "times, distances, and widths must be finite and positive",
            ));
        }
        if !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || self
                .cool_color
                .iter()
                .chain(&self.warm_color)
                .chain(&self.boost_color)
                .any(|v| !v.is_finite() || !(0.0..=4.0).contains(v))
        {
            return Err(MistError::InvalidStyle(
                "opacity must be 0..1 and colors finite linear RGB in 0..4",
            ));
        }
        Ok(())
    }
}

/// Optional presentation-only motion override for a controlled production-render showcase.
#[derive(Component, Debug, Clone, Copy)]
pub struct MistMotion {
    /// If present, move historical samples behind an anchored bubble at world units/second.
    pub conveyor_velocity: Option<Vec3>,
    /// Whether this bubble currently produces fresh samples.
    pub enabled: bool,
}

#[derive(Component)]
/// Internal ownership marker preventing repeat allocation for the same bubble entity.
pub struct AssignedEmitter;

#[derive(Component)]
/// Marker identifying a reusable ribbon render entity.
pub struct MistRibbon;

struct EmitterSlot {
    owner: Option<Entity>,
    render_entity: Entity,
    mesh: Handle<Mesh>,
    history: EmitterHistory,
    virtual_position: Vec3,
    origin: Vec3,
    offset: Vec3,
    radius: f32,
    boosted: bool,
    was_enabled: bool,
    generation: u32,
}

/// Observable bounded renderer statistics for profiling and capture assertions.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct MistStats {
    /// Configured fixed emitter-slot count.
    pub capacity: usize,
    /// Slots currently owned by bubbles or fading after removal.
    pub occupied: usize,
    /// Ribbons with at least two live samples.
    pub visible: usize,
    /// Total retained history samples.
    pub samples: usize,
    /// Total explicit pool exhaustion failures.
    pub overflows: u64,
}

#[derive(Resource)]
/// Persistent renderer allocation owned exclusively by [`MistWakePlugin`].
pub struct MistPool {
    style: MistStyle,
    slots: Vec<EmitterSlot>,
    free: Vec<usize>,
    clock: f32,
    frame: gpu::FrameGpu,
    sequence: u64,
    publication: Arc<Mutex<gpu::Publication>>,
    acknowledged: Vec<gpu::PublishedSlot>,
}

/// Installs production mist rendering, sampling and bounded allocation.
pub struct MistWakePlugin;

impl Plugin for MistWakePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<MistWakeMaterial>::default())
            .init_resource::<MistStats>()
            .init_resource::<lighting::LightAdmission>()
            .add_systems(
                PostUpdate,
                lighting::admit_lights
                    .after(bevy::render::view::VisibilitySystems::CheckVisibility)
                    .after(TransformSystem::TransformPropagate),
            )
            .add_systems(Startup, setup_mist)
            .add_systems(
                PostUpdate,
                update_mist_wake_system.before(TransformSystem::TransformPropagate),
            );
        upload::install(app);
    }
}

fn allocate_pool(
    style: MistStyle,
    commands: &mut Commands<'_, '_>,
    meshes: &mut Assets<Mesh>,
) -> Result<MistPool, MistError> {
    let layout = gpu::PacketLayout::new(style.max_emitters, style.samples_per_emitter)?;
    let frame = gpu::FrameGpu {
        appearance: MistWakeUniformGpu {
            cool: Vec3::from_array(style.cool_color).extend(style.soft_intersection_units),
            warm: Vec3::from_array(style.warm_color).extend(0.0),
            boost: Vec3::from_array(style.boost_color).extend(style.opacity),
            light: Vec4::new(style.reference_illuminance_lux, 0.0, 0.0, 0.0),
        },
        geometry: Vec4::new(style.lifetime_seconds, style.width_radii, 0.0, 0.0),
    };
    let mut slots = Vec::with_capacity(style.max_emitters);
    for index in 0..style.max_emitters {
        let mesh = meshes.add(ribbon::allocate_slot_mesh(
            style.samples_per_emitter,
            index as u32,
        )?);
        let render_entity = commands
            .spawn((
                Mesh3d(mesh.clone()),
                Transform::default(),
                Visibility::Hidden,
                Aabb::default(),
                MistRibbon,
                upload::MistGpuIdentity {
                    slot: index as u32,
                    generation: 0,
                },
                bevy::pbr::NotShadowCaster,
            ))
            .id();
        slots.push(EmitterSlot {
            owner: None,
            render_entity,
            mesh,
            history: EmitterHistory::new(&style)?,
            virtual_position: Vec3::ZERO,
            origin: Vec3::ZERO,
            offset: Vec3::ZERO,
            radius: 1.0,
            boosted: false,
            was_enabled: false,
            generation: 0,
        });
    }
    Ok(MistPool {
        free: (0..style.max_emitters).rev().collect(),
        style,
        slots,
        clock: 0.0,
        frame,
        sequence: 0,
        publication: Arc::new(Mutex::new(gpu::Publication::new(layout))),
        acknowledged: vec![gpu::PublishedSlot::default(); layout.emitters],
    })
}

fn setup_mist(
    mut commands: Commands<'_, '_>,
    root: Option<Res<'_, ResolvedAssetsRoot>>,
    device: Res<'_, RenderDevice>,
    mut meshes: ResMut<'_, Assets<Mesh>>,
    mut materials: ResMut<'_, Assets<MistWakeMaterial>>,
    mut exit: EventWriter<'_, AppExit>,
) {
    let result = (|| -> Result<(MistPool, gpu::GpuResources), Box<dyn std::error::Error>> {
        let owned_root;
        let root = if let Some(root) = root.as_deref() {
            root
        } else {
            owned_root = ResolvedAssetsRoot::resolve()?;
            &owned_root
        };
        let style = MistStyle::load(&root.as_path().join("catalog/mist.ron"))?;
        let pool = allocate_pool(style, &mut commands, &mut meshes)?;
        let layout =
            gpu::PacketLayout::new(pool.style.max_emitters, pool.style.samples_per_emitter)?;
        let resources = gpu::GpuResources::new(&device, layout, Arc::clone(&pool.publication))?;
        let material = materials.add(MistWakeMaterial {
            headers: resources.headers.clone(),
            samples: resources.samples.clone(),
            frame: resources.frame.clone(),
        });
        for slot in &pool.slots {
            commands
                .entity(slot.render_entity)
                .insert(MeshMaterial3d(material.clone()));
        }
        Ok((pool, resources))
    })();
    match result {
        Ok((pool, resources)) => {
            commands.insert_resource(pool);
            commands.insert_resource(resources);
        }
        Err(error) => {
            error!("Mist startup failed: {error}");
            exit.send(AppExit::error());
        }
    }
}

/// Observes world-space motion and updates bounded histories and conservative publication bounds.
#[allow(clippy::type_complexity)]
pub fn update_mist_wake_system(
    mut commands: Commands<'_, '_>,
    time: Res<'_, Time>,
    lab: Option<Res<'_, LabState>>,
    pool: Option<ResMut<'_, MistPool>>,
    stats: Option<ResMut<'_, MistStats>>,
    bubbles: Query<
        '_,
        '_,
        (
            Entity,
            &BubbleInstance,
            &Transform,
            Option<&LabPreviewBubble>,
            Option<&MistMotion>,
            Has<AssignedEmitter>,
        ),
        Without<crate::smog::BouncingSmogBubble>,
    >,
    mut ribbons: Query<
        '_,
        '_,
        (
            &mut Transform,
            &mut Visibility,
            &mut Aabb,
            &mut upload::MistGpuIdentity,
        ),
        (Without<BubbleInstance>, With<MistRibbon>),
    >,
    mut exit: EventWriter<'_, AppExit>,
    timing: Option<ResMut<'_, MistCpuTiming>>,
    smog_config: Option<Res<'_, crate::smog::SmogConfig>>,
) {
    if smog_config.as_ref().is_some_and(|c| c.enabled) {
        for (_, mut vis, _, _) in &mut ribbons {
            if *vis != Visibility::Hidden {
                *vis = Visibility::Hidden;
            }
        }
        return;
    }

    let timer = benchmark::CpuTimer::new(timing.is_some());
    let Some(mut pool) = pool else {
        return;
    };
    let Some(mut stats) = stats else {
        return;
    };
    let dt = time.delta_secs_f64();
    pool.clock += time.delta_secs();
    pool.frame.appearance.warm.w = pool.clock;
    let Some(sequence) = pool.sequence.checked_add(1) else {
        error!("{}", gpu::MistGpuError::CounterExhausted("packet sequence"));
        exit.send(AppExit::error());
        return;
    };
    pool.sequence = sequence;
    let shared_publication = Arc::clone(&pool.publication);
    let Ok(publication) = shared_publication.lock() else {
        error!("{}", gpu::MistGpuError::PublicationPoisoned);
        exit.send(AppExit::error());
        return;
    };
    if let Some(error) = &publication.error {
        error!("{error}");
        exit.send(AppExit::error());
        return;
    }
    pool.acknowledged.copy_from_slice(&publication.slots);
    drop(publication);
    // Retired histories may finish fading, but they no longer reserve emitter capacity.
    // A replacement owner reuses the complete slot immediately and resets its history.
    // This keeps a 400-unit scene reset within the same fixed 512-slot budget.
    {
        let MistPool { slots, free, .. } = &mut *pool;
        for (index, slot) in slots.iter_mut().enumerate() {
            if slot.owner.is_some_and(|owner| bubbles.get(owner).is_err()) {
                slot.owner = None;
                slot.was_enabled = false;
                free.push(index);
            }
        }
    }
    for (entity, bubble, transform, _, _, assigned) in &bubbles {
        if assigned || bubble.0.shape != ShapeKind::UnitSphere {
            continue;
        }
        let Some(index) = pool.free.pop() else {
            stats.overflows += 1;
            error!("{}", MistError::CapacityExhausted);
            exit.send(AppExit::error());
            return;
        };
        let slot = &mut pool.slots[index];
        let Some(generation) = slot.generation.checked_add(1) else {
            error!(
                "{}",
                gpu::MistGpuError::CounterExhausted("owner generation")
            );
            exit.send(AppExit::error());
            return;
        };
        slot.generation = generation;
        slot.owner = Some(entity);
        slot.history.reset();
        slot.virtual_position = transform.translation;
        slot.origin = transform.translation;
        slot.offset = Vec3::ZERO;
        slot.was_enabled = false;
        commands.entity(entity).insert(AssignedEmitter);
    }
    let MistPool {
        style,
        slots,
        acknowledged,
        ..
    } = &mut *pool;
    stats.capacity = slots.len();
    stats.occupied = 0;
    stats.visible = 0;
    stats.samples = 0;
    for (index, slot) in slots.iter_mut().enumerate() {
        let result = if let Some((_, bubble, transform, preview, override_motion, _)) =
            slot.owner.and_then(|owner| bubbles.get(owner).ok())
        {
            slot.origin = transform.translation;
            slot.radius = bubble.0.radius;
            slot.boosted = bubble.0.speed_boosted;
            let conveyor = override_motion
                .and_then(|m| m.conveyor_velocity)
                .or_else(|| {
                    if preview.is_some() {
                        lab.as_ref().map(|lab| Vec3::X * lab.speed)
                    } else {
                        None
                    }
                });
            let smog_replaces_mist = smog_config.as_ref().is_some_and(|c| c.enabled);
            let enabled = !smog_replaces_mist
                && override_motion.is_none_or(|m| m.enabled)
                && (preview.is_none()
                    || lab.as_ref().is_none_or(|l| {
                        l.opt_mist_wake && !(l.smog_bouncing_active && l.opt_smog_volume)
                    }));
            if enabled {
                if !slot.was_enabled {
                    slot.history.reset();
                    slot.virtual_position = slot.origin;
                }
                slot.was_enabled = true;
                let observed = if let Some(velocity) = conveyor {
                    slot.virtual_position += velocity * time.delta_secs();
                    slot.offset = slot.origin - slot.virtual_position;
                    slot.virtual_position
                } else {
                    slot.offset = Vec3::ZERO;
                    slot.origin
                };
                slot.history.advance(observed, slot.radius, dt, style)
            } else {
                slot.was_enabled = false;
                slot.history.age(dt, style)
            }
        } else {
            slot.history.age(dt, style)
        };
        if let Err(error) = result {
            error!("Mist motion failed: {error}");
            exit.send(AppExit::error());
            return;
        }
        let visible = slot.history.len() > 1;
        if let Ok((mut transform, mut visibility, mut bounds, mut identity)) =
            ribbons.get_mut(slot.render_entity)
        {
            identity.generation = slot.generation;
            let acknowledged = acknowledged[index];
            let same_generation = acknowledged.generation == slot.generation;
            *visibility = if same_generation && acknowledged.count > 1 {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
            let anchor = if same_generation {
                acknowledged.origin
            } else {
                slot.origin
            };
            transform.translation = anchor;
            let mut minimum = slot.origin;
            let mut maximum = slot.origin;
            for knot in 0..slot.history.len() {
                if let Some(sample) = slot.history.sample(knot) {
                    let center = sample.position + slot.offset;
                    minimum = minimum.min(center);
                    maximum = maximum.max(center);
                }
            }
            let excursion = Vec3::splat(ribbon::maximum_sample_excursion(slot.radius, style));
            minimum -= excursion;
            maximum += excursion;
            if same_generation && acknowledged.count > 1 {
                minimum = minimum.min(acknowledged.minimum);
                maximum = maximum.max(acknowledged.maximum);
            }
            *bounds = Aabb::from_min_max(minimum - anchor, maximum - anchor);
        }
        if slot.owner.is_some() || !slot.history.is_empty() {
            stats.occupied += 1;
            stats.visible += usize::from(visible);
            stats.samples += slot.history.len();
        }
    }
    timer.finish(timing.map(ResMut::into_inner));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::time::TimeUpdateStrategy;
    use sim_types::{
        AnimState, BubbleRenderInstance, EntityId, PlayerId, SilhouetteClass, UnitTypeId,
        WeaponVisualKind,
    };
    use std::time::Duration;

    fn instance(index: u32) -> BubbleInstance {
        BubbleInstance(BubbleRenderInstance {
            entity_id: EntityId(index),
            shape: ShapeKind::UnitSphere,
            silhouette: SilhouetteClass::LightProngs,
            unit_type: UnitTypeId(0),
            owner: PlayerId(1),
            world_pos: [0.0; 3],
            ground_z: 0.0,
            radius: 1.0,
            velocity: [1.0, 0.0],
            heading_angle_rad: 0.0,
            aim_angle_rad: 0.0,
            is_attacking: false,
            weapon_visual: WeaponVisualKind::Hitscan,
            attack_target_pos: None,
            anim_state: AnimState::Walk,
            anim_progress: 0.0,
            hp_ratio: 1.0,
            shield_ratio: 1.0,
            shield_hit_flash: 0.0,
            hull_hit_angle_rad: 0.0,
            hull_hit_flash: 0.0,
            build_or_queue_progress: None,
            selected: false,
            cloaked: false,
            speed_boosted: false,
        })
    }

    fn state_app() -> Result<App, Box<dyn std::error::Error>> {
        let mut app = App::new();
        // Real production allocation/update systems without a renderer: this test verifies
        // lifetime/resource behavior only. GPU and screenshot evidence comes from mist_preview.
        app.add_plugins(MinimalPlugins)
            .insert_resource(ResolvedAssetsRoot::from_directory(
                &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
            )?)
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                1.0 / 60.0,
            )))
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<MistWakeMaterial>>()
            .init_resource::<MistStats>()
            .init_resource::<lighting::LightAdmission>()
            .add_systems(
                PostUpdate,
                lighting::admit_lights
                    .after(bevy::render::view::VisibilitySystems::CheckVisibility)
                    .after(TransformSystem::TransformPropagate),
            )
            .add_event::<AppExit>()
            .add_systems(PostUpdate, update_mist_wake_system);
        let style = MistStyle::load(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/catalog/mist.ron"),
        )?;
        let pool = app.world_mut().run_system_once(
            move |mut commands: Commands<'_, '_>, mut meshes: ResMut<'_, Assets<Mesh>>| {
                allocate_pool(style.clone(), &mut commands, &mut meshes)
            },
        )??;
        app.insert_resource(pool);
        Ok(app)
    }

    fn spawn_wave(app: &mut App) -> Vec<Entity> {
        (0..400)
            .map(|index| {
                app.world_mut()
                    .spawn((
                        instance(index),
                        Transform::from_xyz(index as f32, 1.0, 0.0),
                        MistMotion {
                            conveyor_velocity: Some(Vec3::X * 2.5),
                            enabled: true,
                        },
                    ))
                    .id()
            })
            .collect()
    }

    #[test]
    fn four_hundred_owner_replacements_reuse_resources_and_entity_generations(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app = state_app()?;
        let first = spawn_wave(&mut app);
        for _ in 0..90 {
            app.update();
        }
        let meshes = app.world().resource::<Assets<Mesh>>().len();
        let materials = app.world().resource::<Assets<MistWakeMaterial>>().len();
        let entities = app.world().entities().len();
        assert_eq!(app.world().resource::<MistStats>().visible, 400);
        for entity in &first {
            assert!(app.world_mut().despawn(*entity));
        }
        let second = spawn_wave(&mut app);
        assert!(second.iter().any(|new| first
            .iter()
            .any(|old| old.index() == new.index() && old != new)));
        app.update();
        let stats = app.world().resource::<MistStats>();
        assert_eq!(stats.occupied, 400);
        assert_eq!(stats.overflows, 0);
        assert_eq!(
            stats.samples, 0,
            "replacement generations must start with empty histories"
        );
        for _ in 0..90 {
            app.update();
        }
        assert_eq!(app.world().resource::<Assets<Mesh>>().len(), meshes);
        assert_eq!(
            app.world().resource::<Assets<MistWakeMaterial>>().len(),
            materials
        );
        assert_eq!(app.world().entities().len(), entities);
        for entity in second {
            assert!(app.world_mut().despawn(entity));
        }
        for _ in 0..90 {
            app.update();
        }
        let pool = app.world().resource::<MistPool>();
        assert_eq!(pool.free.len(), pool.slots.len());
        assert!(pool
            .slots
            .iter()
            .all(|slot| slot.owner.is_none() && slot.history.is_empty()));
        assert_eq!(app.world().resource::<MistStats>().visible, 0);
        Ok(())
    }

    #[test]
    fn disabling_and_reenabling_cannot_bridge_stale_positions(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut app = state_app()?;
        let entity = app
            .world_mut()
            .spawn((
                instance(1),
                Transform::from_xyz(0.0, 1.0, 0.0),
                MistMotion {
                    conveyor_velocity: Some(Vec3::X * 2.5),
                    enabled: true,
                },
            ))
            .id();
        for _ in 0..60 {
            app.update();
        }
        app.world_mut().entity_mut(entity).insert(MistMotion {
            conveyor_velocity: None,
            enabled: false,
        });
        app.world_mut()
            .entity_mut(entity)
            .insert(Transform::from_xyz(5.0, 1.0, 0.0));
        app.update();
        app.world_mut().entity_mut(entity).insert(MistMotion {
            conveyor_velocity: None,
            enabled: true,
        });
        app.update();
        assert_eq!(app.world().resource::<MistStats>().samples, 0);
        app.world_mut()
            .entity_mut(entity)
            .insert(Transform::from_xyz(5.5, 1.0, 0.0));
        app.update();
        let pool = app.world().resource::<MistPool>();
        let history = &pool
            .slots
            .iter()
            .find(|slot| slot.owner == Some(entity))
            .ok_or("missing owner")?
            .history;
        assert_ne!(history.len(), 0);
        assert!(history.sample(0).ok_or("missing sample")?.position.x > 4.0);
        Ok(())
    }
}
