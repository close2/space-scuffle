//! Exact main-world source extraction for the bounded Swarm experiment.
//!
//! Only actual visible geometry is inventoried. Unsupported material kinds remain
//! explicit limitations; absence from the source table never means proven stability.

use super::model::{AuditError, LogicalIdentity, MaterialSource, SourceFrame, SourceItem};
use bevy::prelude::*;
use bevy::render::primitives::Aabb;
use bevy::render::view::ViewVisibility;
use client::{BubbleDomeMaterial, BubbleInstance, LabPreviewBubble, LiveBattleBubble, SwarmBubble};

/// Captures actual visible Swarm and background CPU sources, or None outside that scene.
///
/// Evidence is gathered in a fixed order: the frame header, every visible mesh with its
/// material and assets, then the lights and cameras of the scene environment.
pub(super) fn capture(
    world: &mut World,
    assets: &mut super::assets::Inventory,
    limit: usize,
) -> Result<Option<SourceFrame>, AuditError> {
    if !world
        .get_resource::<client::LabState>()
        .is_some_and(|lab| lab.current_tab == client::LabTab::SwarmStressTest && lab.swarm_active)
    {
        return Ok(None);
    }
    let mut frame = frame_header(world)?;
    let images = inventory_meshes(world, assets, limit, &mut frame)?;
    for image in images {
        frame.assets.push(assets.image(world, &image, limit)?);
    }
    frame.assets.extend(assets.shaders(world, limit)?);
    frame.assets.sort_unstable();
    frame.assets.dedup();
    frame.items.sort_by_key(|item| item.main_entity);
    record_environment(world, &mut frame);
    Ok(Some(frame))
}

/// Starts a source frame with the virtual clock, clear colour, ambient light, the number of
/// instantiated Swarm shells and the controlled fixture, if one is declared.
fn frame_header(world: &mut World) -> Result<SourceFrame, AuditError> {
    let time = world.resource::<Time>();
    Ok(SourceFrame {
        elapsed_nanos: u64::try_from(time.elapsed().as_nanos())
            .map_err(|_| AuditError::Exhausted)?,
        wrapped_time_bits: time.elapsed_secs_wrapped().to_bits(),
        delta_bits: time.delta_secs().to_bits(),
        scene: "SwarmStressTest".into(),
        environment: vec![
            format!("clear={:?}", world.get_resource::<ClearColor>()),
            format!("ambient={:?}", world.get_resource::<AmbientLight>()),
        ],
        items: vec![],
        limitations: vec![],
        assets: vec![],
        instantiated_swarm_shells: world
            .query_filtered::<Entity, With<SwarmBubble>>()
            .iter(world)
            .count(),
        fixture: world
            .get_resource::<super::fixture::ControlledFixture>()
            .map(|fixture| format!("controlled two-shell experiment: {:?}", fixture.0)),
    })
}

/// Which shell scenes one bubble entity is tagged as belonging to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ShellDomain {
    VisualLab,
    Swarm,
    Battle,
    Smog,
}

/// The tags of one mesh, one flag per [`ShellDomain`].
#[derive(Clone, Copy)]
struct ShellTags([bool; 4]);

impl ShellTags {
    /// Whether the mesh carries the tag of `domain`.
    const fn has(self, domain: ShellDomain) -> bool {
        self.0[domain as usize]
    }
}

/// Inventories every visible mesh into `frame.items`, with its mesh asset, and returns the
/// material images still to be inventoried. Unsupported cases become frame limitations.
fn inventory_meshes(
    world: &mut World,
    assets: &mut super::assets::Inventory,
    limit: usize,
    frame: &mut SourceFrame,
) -> Result<Vec<Handle<Image>>, AuditError> {
    let mut query = world.query::<(
        Entity,
        &GlobalTransform,
        &Mesh3d,
        Option<&Aabb>,
        Option<&ViewVisibility>,
        Option<&BubbleInstance>,
        Option<&LabPreviewBubble>,
        Option<&SwarmBubble>,
        Option<&LiveBattleBubble>,
        Option<&client::smog::BouncingSmogBubble>,
        Option<&MeshMaterial3d<BubbleDomeMaterial>>,
        Option<&MeshMaterial3d<StandardMaterial>>,
    )>();
    let mut images = Vec::new();
    let mut identities = std::collections::BTreeSet::new();
    for (
        entity,
        transform,
        mesh,
        bounds,
        visibility,
        instance,
        lab,
        swarm,
        live,
        smog,
        shell,
        standard,
    ) in query.iter(world)
    {
        let Some(visibility) = visibility else {
            frame
                .limitations
                .push(format!("mesh {entity:?} missing visibility"));
            continue;
        };
        if !visibility.get() {
            continue;
        }
        let matrix = transform.compute_matrix();
        if !matrix.is_finite() {
            return Err(AuditError::Source(format!(
                "nonfinite transform {entity:?}"
            )));
        }
        let bounds = bounds_bits(entity, bounds)?;
        if bounds.is_none() {
            frame
                .limitations
                .push(format!("visible mesh {entity:?} missing bounds"));
        }
        if shell.is_some() && standard.is_some() {
            frame.limitations.push(format!("multiple material associations on visible mesh {entity:?}; adapter precedence cannot establish all draw sources"));
        }
        let domains = ShellTags([
            lab.is_some(),
            swarm.is_some(),
            live.is_some(),
            smog.is_some(),
        ]);
        let logical = logical_identity(entity, instance, domains, frame, &mut identities)?;
        frame.assets.push(assets.mesh(world, &mesh.0, limit)?);
        let material = material_source(
            world,
            entity,
            matrix,
            instance,
            domains.has(ShellDomain::Swarm),
            (shell, standard),
            &mut images,
        )?
        .unwrap_or_else(|| {
            frame.limitations.push(format!(
                "visible material adapter unsupported for {entity:?}"
            ));
            MaterialSource::Unsupported {
                reason: "no shell or standard material association",
            }
        });
        frame.items.push(SourceItem {
            main_entity: entity.to_bits(),
            logical,
            owner: instance.map(|value| value.0.owner.0),
            world_from_local: matrix.to_cols_array().map(f32::to_bits),
            bounds,
            mesh: format!("{:?}", mesh.0.id()),
            material,
        });
    }
    Ok(images)
}

/// The bit patterns of a finite, non-negative axis-aligned box (center XYZ, then half
/// extents XYZ, in local mesh units), or None for a mesh without bounds.
fn bounds_bits(entity: Entity, bounds: Option<&Aabb>) -> Result<Option<[u32; 6]>, AuditError> {
    bounds
        .map(|bounds| {
            if !bounds.center.is_finite()
                || !bounds.half_extents.is_finite()
                || bounds.half_extents.cmplt(bevy::math::Vec3A::ZERO).any()
            {
                return Err(AuditError::Source(format!("invalid bounds {entity:?}")));
            }
            Ok([
                bounds.center.x.to_bits(),
                bounds.center.y.to_bits(),
                bounds.center.z.to_bits(),
                bounds.half_extents.x.to_bits(),
                bounds.half_extents.y.to_bits(),
                bounds.half_extents.z.to_bits(),
            ])
        })
        .transpose()
}

/// The logical shell identity of a bubble mesh, or None for a mesh with no bubble instance.
///
/// A bubble must belong to exactly one shell domain, and no identity may repeat within the
/// frame (`identities` holds those already seen).
fn logical_identity(
    entity: Entity,
    instance: Option<&BubbleInstance>,
    domains: ShellTags,
    frame: &mut SourceFrame,
    identities: &mut std::collections::BTreeSet<LogicalIdentity>,
) -> Result<Option<LogicalIdentity>, AuditError> {
    let Some(instance) = instance else {
        return Ok(None);
    };
    if domains.0.into_iter().filter(|flag| *flag).count() != 1 {
        return Err(AuditError::Source(format!(
            "ambiguous shell domain {entity:?}"
        )));
    }
    let identity = LogicalIdentity {
        domain: if domains.has(ShellDomain::Swarm) {
            "Swarm"
        } else if domains.has(ShellDomain::VisualLab) {
            "VisualLab"
        } else if domains.has(ShellDomain::Smog) {
            "Smog"
        } else {
            "Battle"
        },
        entity: instance.0.entity_id.0,
        generation: 0,
    };
    if domains.has(ShellDomain::Battle) {
        frame.limitations.push("live battle shell unexpectedly visible in Swarm audit; committed generation unresolved".into());
    }
    if !identities.insert(identity.clone()) {
        return Err(AuditError::Source(
            "duplicate visible logical identity".into(),
        ));
    }
    Ok(Some(identity))
}

/// Describes the material of one visible mesh and queues its images onto `images`, or
/// returns None for a mesh with neither a shell nor a standard material.
///
/// A shell takes precedence over a standard material. Under the controlled fixture, a Swarm
/// shell's transform, snapshot and uniform centers must agree.
fn material_source(
    world: &World,
    entity: Entity,
    matrix: Mat4,
    instance: Option<&BubbleInstance>,
    swarm: bool,
    (shell, standard): (
        Option<&MeshMaterial3d<BubbleDomeMaterial>>,
        Option<&MeshMaterial3d<StandardMaterial>>,
    ),
    images: &mut Vec<Handle<Image>>,
) -> Result<Option<MaterialSource>, AuditError> {
    if let Some(shell) = shell {
        let material = world
            .resource::<Assets<BubbleDomeMaterial>>()
            .get(&shell.0)
            .ok_or_else(|| AuditError::NotReady(format!("shell material {entity:?}")))?;
        if world.contains_resource::<super::fixture::ControlledFixture>() && swarm {
            let instance = instance
                .ok_or_else(|| AuditError::Source("controlled shell snapshot absent".into()))?;
            super::fixture::validate_centers(
                matrix.transform_point3(Vec3::ZERO),
                instance.0.world_pos,
                material.uniform.world_pos,
            )?;
        }
        if let Some(image) = &material.video_texture {
            images.push(image.clone());
        } else {
            return Err(AuditError::Source(
                "actual shell has no video texture".into(),
            ));
        }
        Ok(Some(MaterialSource::Shell {
            id: format!("{:?}", shell.0.id()),
            uniform: bytemuck::bytes_of(&material.uniform).to_vec(),
            image: format!("{:?}", material.video_texture.as_ref().map(Handle::id)),
        }))
    } else if let Some(standard) = standard {
        let material = world
            .resource::<Assets<StandardMaterial>>()
            .get(&standard.0)
            .ok_or_else(|| AuditError::NotReady(format!("standard material {entity:?}")))?;
        for image in [
            &material.base_color_texture,
            &material.emissive_texture,
            &material.metallic_roughness_texture,
            &material.normal_map_texture,
            &material.occlusion_texture,
            &material.depth_map,
        ]
        .into_iter()
        .flatten()
        {
            images.push(image.clone());
        }
        Ok(Some(MaterialSource::Standard {
            id: format!("{:?}", standard.0.id()),
            description: format!("{material:?}"),
        }))
    } else {
        Ok(None)
    }
}

/// Appends directional lights, active camera settings, point lights and spot lights to the
/// frame environment, then sorts it so query order cannot change the evidence.
fn record_environment(world: &mut World, frame: &mut SourceFrame) {
    let mut lights = world.query::<(
        Entity,
        &GlobalTransform,
        &DirectionalLight,
        Option<&ViewVisibility>,
    )>();
    for (entity, transform, light, visible) in lights.iter(world) {
        frame.environment.push(format!(
            "directional {entity:?} matrix={:?} light={light:?} visible={:?}",
            transform.compute_matrix().to_cols_array().map(f32::to_bits),
            visible.map(|value| value.get())
        ));
    }
    let mut cameras = world.query::<(
        Entity,
        &Camera,
        &Projection,
        Option<&bevy::core_pipeline::tonemapping::Tonemapping>,
        Option<&bevy::core_pipeline::bloom::Bloom>,
    )>();
    for (entity, camera, projection, tone, bloom) in cameras.iter(world) {
        if camera.is_active {
            frame.environment.push(format!(
                "view extras {entity:?}: fog={:?}; exposure={:?}; msaa={:?}",
                world.get::<DistanceFog>(entity),
                world
                    .get::<bevy::render::camera::Exposure>(entity)
                    .map(|value| value.ev100.to_bits()),
                world.get::<Msaa>(entity)
            ));
            frame.environment.push(format!("camera {entity:?} {projection:?} target={:?} viewport={:?} hdr={} tone={tone:?} bloom={:?}", camera.target, camera.viewport, camera.hdr, bloom.map(|value| format!("{:?}", value.as_partial_reflect()))));
        }
    }
    let mut points = world.query::<(Entity, &GlobalTransform, &PointLight)>();
    for (entity, transform, light) in points.iter(world) {
        frame.environment.push(format!(
            "point {entity:?}: matrix={:?}; {light:?}",
            transform.compute_matrix().to_cols_array().map(f32::to_bits)
        ));
    }
    let mut spots = world.query::<(Entity, &GlobalTransform, &SpotLight)>();
    for (entity, transform, light) in spots.iter(world) {
        frame.environment.push(format!(
            "spot {entity:?}: matrix={:?}; {light:?}",
            transform.compute_matrix().to_cols_array().map(f32::to_bits)
        ));
    }
    frame.environment.sort();
}
