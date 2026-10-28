//! Actual engine-view light packing and dynamic-offset readback for the mist shader.
//!
//! The scene uses ordinary cameras, parent transforms, visibility and render layers.
//! The compute oracle binds Bevy's real prepared `LightMeta` buffer, never an authored
//! replacement, and calls the exact lighting function read from the shipped shader.

use bevy::pbr::{GpuLights, LightMeta, ViewLightsUniformOffset, MAX_DIRECTIONAL_LIGHTS};
use bevy::prelude::*;
use bevy::render::camera::RenderTarget;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::sync_world::MainEntity;
use bevy::render::view::RenderLayers;
use bevy::render::RenderApp;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Reading {
    axes: [[f32; 4]; 3],
    counts: [u32; 4],
    colors: [[f32; 4]; MAX_DIRECTIONAL_LIGHTS],
    directions: [[f32; 4]; MAX_DIRECTIONAL_LIGHTS],
}

const ENTRY: &str = r"
struct Reading {
    axes: array<vec4<f32>, 3>,
    counts: vec4<u32>,
    colors: array<vec4<f32>, #{MAX_DIRECTIONAL_LIGHTS}>,
    directions: array<vec4<f32>, #{MAX_DIRECTIONAL_LIGHTS}>,
};
@group(1) @binding(0) var<storage, read_write> result: Reading;
@compute @workgroup_size(1)
fn read_lights() {
    result.axes[0] = vec4<f32>(mist_direct_light(vec3<f32>(1.0, 0.0, 0.0), 10000.0), 0.0);
    result.axes[1] = vec4<f32>(mist_direct_light(vec3<f32>(0.0, 1.0, 0.0), 10000.0), 0.0);
    result.axes[2] = vec4<f32>(mist_direct_light(vec3<f32>(0.0, 0.0, 1.0), 10000.0), 0.0);
    var eligible = 0u;
    for (var i = 0u; i < lights.n_directional_lights; i += 1u) {
        let light = lights.directional_lights[i];
        if light.skip == 0u { eligible += 1u; }
        result.colors[i] = light.color;
        result.directions[i] = vec4<f32>(light.direction_to_light, f32(light.skip));
    }
    result.counts = vec4<u32>(lights.n_directional_lights, eligible, 0u, 0u);
}
";

struct Oracle {
    pipeline: CachedComputePipelineId,
    lights_layout: BindGroupLayout,
    result_group: BindGroup,
    output: Buffer,
    readback: Buffer,
}

impl Oracle {
    fn new(app: &mut App) -> Result<Self, Box<dyn std::error::Error>> {
        let source = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../assets/shaders/mist_wake.wgsl"),
        )?;
        let begin = source
            .find("fn mist_direct_light(")
            .ok_or("shipped light function missing")?;
        let end = source[begin..]
            .find("// Convert straight")
            .ok_or("shipped helper boundary missing")?
            + begin;
        let source = format!(
            "#import bevy_pbr::mesh_view_bindings::lights\n{}\n{ENTRY}",
            &source[begin..end]
        );
        let shader = app
            .world_mut()
            .resource_mut::<Assets<Shader>>()
            .add(Shader::from_wgsl(
                source,
                "actual-mist-view-light-oracle.wgsl",
            ));
        let device = app.world().resource::<RenderDevice>();
        let lights_layout = device.create_bind_group_layout(
            "actual view light oracle",
            &[BindGroupLayoutEntry {
                binding: 1,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: Some(GpuLights::min_size()),
                },
                count: None,
            }],
        );
        let result_layout = device.create_bind_group_layout(
            "view light oracle output",
            &[BindGroupLayoutEntry {
                binding: 0,
                visibility: ShaderStages::COMPUTE,
                ty: BindingType::Buffer {
                    ty: BufferBindingType::Storage { read_only: false },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        );
        let size = u64::try_from(size_of::<Reading>())?;
        let output = device.create_buffer(&BufferDescriptor {
            label: Some("actual light readback output"),
            size,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&BufferDescriptor {
            label: Some("actual light readback mapping"),
            size,
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let result_group = device.create_bind_group(
            "view light readback output",
            &result_layout,
            &[BindGroupEntry {
                binding: 0,
                resource: output.as_entire_binding(),
            }],
        );
        let pipeline = app
            .sub_app(RenderApp)
            .world()
            .resource::<PipelineCache>()
            .queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("actual view-bound mist lighting".into()),
                layout: vec![lights_layout.clone(), result_layout],
                push_constant_ranges: vec![],
                shader,
                shader_defs: vec![ShaderDefVal::UInt(
                    "MAX_DIRECTIONAL_LIGHTS".into(),
                    u32::try_from(MAX_DIRECTIONAL_LIGHTS)?,
                )],
                entry_point: "read_lights".into(),
                zero_initialize_workgroup_memory: true,
            });
        Ok(Self {
            pipeline,
            lights_layout,
            result_group,
            output,
            readback,
        })
    }

    fn ready(&self, app: &mut App, cameras: &[Entity]) -> TestResult {
        let started = Instant::now();
        loop {
            app.update();
            let world = app.sub_app_mut(RenderApp).world_mut();
            let count = world
                .query::<(MainEntity, &ViewLightsUniformOffset)>()
                .iter(world)
                .filter(|(entity, _)| cameras.contains(entity))
                .count();
            let cache = world.resource::<PipelineCache>();
            if count == cameras.len() && cache.get_compute_pipeline(self.pipeline).is_some() {
                return Ok(());
            }
            if let CachedPipelineState::Err(error) = cache.get_compute_pipeline_state(self.pipeline)
            {
                if !matches!(
                    error,
                    PipelineCacheError::ShaderNotLoaded(_)
                        | PipelineCacheError::ShaderImportNotYetAvailable
                ) {
                    return Err(format!("actual view light pipeline failed: {error}").into());
                }
            }
            if started.elapsed() > Duration::from_secs(30) {
                return Err("actual light/view readiness timeout".into());
            }
            std::thread::yield_now();
        }
    }

    fn read(
        &self,
        app: &mut App,
        camera: Entity,
    ) -> Result<(u32, Reading), Box<dyn std::error::Error>> {
        let world = app.sub_app_mut(RenderApp).world_mut();
        let offset = world
            .query::<(MainEntity, &ViewLightsUniformOffset)>()
            .iter(world)
            .find(|(entity, _)| *entity == camera)
            .map(|(_, value)| value.offset)
            .ok_or("actual camera light offset missing")?;
        let device = world.resource::<RenderDevice>();
        let queue = world.resource::<RenderQueue>();
        let lights = world.resource::<LightMeta>();
        let buffer = lights
            .view_gpu_lights
            .buffer()
            .ok_or("actual light buffer missing")?;
        assert_eq!(
            offset % device.limits().min_uniform_buffer_offset_alignment,
            0
        );
        assert!(u64::from(offset) + GpuLights::min_size().get() <= buffer.size());
        // Binding begins at byte zero with exactly one GpuLights-sized range. The
        // actual per-view dynamic offset is supplied once, only at set_bind_group.
        let group = device.create_bind_group(
            "real prepared view lights",
            &self.lights_layout,
            &[BindGroupEntry {
                binding: 1,
                resource: lights
                    .view_gpu_lights
                    .binding()
                    .ok_or("actual light binding missing")?,
            }],
        );
        let cache = world.resource::<PipelineCache>();
        let pipeline = cache
            .get_compute_pipeline(self.pipeline)
            .ok_or("light pipeline not ready")?;
        let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
            label: Some("actual view light compute oracle"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                label: Some("actual view light compute oracle"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &*group, &[offset]);
            pass.set_bind_group(1, &*self.result_group, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(
            &self.output,
            0,
            &self.readback,
            0,
            u64::try_from(size_of::<Reading>())?,
        );
        queue.submit([encoder.finish()]);
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.readback
            .slice(..)
            .map_async(MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        let start = Instant::now();
        loop {
            device.poll(Maintain::Poll);
            match receiver.try_recv() {
                Ok(result) => {
                    result?;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(error) => return Err(error.into()),
            }
            if start.elapsed() > Duration::from_secs(30) {
                return Err("actual light readback timeout".into());
            }
            std::thread::yield_now();
        }
        let value = *bytemuck::from_bytes::<Reading>(&self.readback.slice(..).get_mapped_range());
        self.readback.unmap();
        eprintln!(
            "real view={camera:?} offset={offset} buffer={:?} bytes={} counts={:?}",
            buffer.id(),
            buffer.size(),
            value.counts
        );
        Ok((offset, value))
    }
}

fn camera(app: &mut App, layer: usize, order: isize) -> Entity {
    let mut image = Image::new_fill(
        Extent3d {
            width: 32,
            height: 32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(image);
    app.world_mut()
        .spawn((
            Camera3d::default(),
            Camera {
                target: RenderTarget::Image(target),
                order,
                ..default()
            },
            Transform::from_xyz(0.0, 2.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            RenderLayers::from_layers(&[0, layer]),
        ))
        .id()
}

fn assert_rgb(actual: [f32; 4], expected: Vec3) {
    assert!(
        Vec3::new(actual[0], actual[1], actual[2]).abs_diff_eq(expected, 0.00002),
        "actual={actual:?}, expected={expected}"
    );
}

#[test]
fn actual_engine_view_lights_use_color_lux_world_direction_and_dynamic_offsets() -> TestResult {
    let mut app = App::new();
    app.add_plugins(
        crate::presentation::GamePresentationPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>()
            .disable::<bevy::render::pipelined_rendering::PipelinedRenderingPlugin>(),
    );
    app.finish();
    app.cleanup();
    let result = verify_scene(&mut app);
    crate::render_compatibility::finish_rendering(&mut app)?;
    result
}

fn verify_scene(app: &mut App) -> TestResult {
    let camera_a = camera(app, 1, 0);
    let camera_b = camera(app, 2, 1);
    let direction = Vec3::new(1.0, 2.0, 3.0).normalize();
    let parent = app
        .world_mut()
        .spawn((Transform::default(), Visibility::Inherited))
        .id();
    let white = app
        .world_mut()
        .spawn((
            DirectionalLight {
                illuminance: 10_000.0,
                ..default()
            },
            Transform::from_rotation(Quat::from_rotation_arc(Vec3::Z, direction)),
            RenderLayers::layer(1),
        ))
        .id();
    app.world_mut().entity_mut(parent).add_child(white);
    let red = app
        .world_mut()
        .spawn((
            DirectionalLight {
                color: Color::linear_rgb(1.0, 0.0, 0.0),
                illuminance: 20_000.0,
                ..default()
            },
            Transform::from_rotation(Quat::from_rotation_arc(Vec3::Z, Vec3::Y)),
            RenderLayers::layer(2),
        ))
        .id();
    let oracle = Oracle::new(app)?;
    oracle.ready(app, &[camera_a, camera_b])?;
    let (offset_a, a) = oracle.read(app, camera_a)?;
    let (offset_b, b) = oracle.read(app, camera_b)?;
    assert_ne!(offset_a, offset_b);
    assert_eq!(a.counts[..2], [2, 1]);
    assert_eq!(b.counts[..2], [2, 1]);
    assert_rgb(a.axes[0], Vec3::splat(direction.x));
    assert_rgb(b.axes[1], Vec3::new(2.0, 0.0, 0.0));
    // Parent propagation must change world X lighting; rotation around world Y
    // would not test direction if the initial vector were parallel to world Y.
    app.world_mut()
        .entity_mut(parent)
        .insert(Transform::from_rotation(Quat::from_rotation_y(
            std::f32::consts::FRAC_PI_2,
        )));
    oracle.ready(app, &[camera_a, camera_b])?;
    let (_, rotated) = oracle.read(app, camera_a)?;
    assert_rgb(rotated.axes[0], Vec3::splat(direction.z));
    app.world_mut()
        .entity_mut(parent)
        .insert(Visibility::Hidden);
    oracle.ready(app, &[camera_a, camera_b])?;
    let (_, parent_hidden) = oracle.read(app, camera_a)?;
    assert_eq!(parent_hidden.counts[..2], [1, 0]);
    assert_rgb(parent_hidden.axes[0], Vec3::ZERO);
    app.world_mut()
        .entity_mut(parent)
        .insert(Visibility::Inherited);
    oracle.ready(app, &[camera_a, camera_b])?;
    let (_, parent_restored) = oracle.read(app, camera_a)?;
    assert_eq!(parent_restored.counts[..2], [2, 1]);
    assert_rgb(parent_restored.axes[0], Vec3::splat(direction.z));
    app.world_mut()
        .entity_mut(camera_a)
        .insert(RenderLayers::layer(0));
    app.world_mut()
        .entity_mut(camera_b)
        .insert(RenderLayers::from_layers(&[0, 1, 2]));
    oracle.ready(app, &[camera_a, camera_b])?;
    let (_, empty_view) = oracle.read(app, camera_a)?;
    let (_, both) = oracle.read(app, camera_b)?;
    assert_eq!(empty_view.counts[..2], [2, 0]);
    assert_eq!(both.counts[..2], [2, 2]);
    assert_rgb(empty_view.axes[1], Vec3::ZERO);
    app.world_mut().entity_mut(white).insert(Visibility::Hidden);
    app.world_mut().entity_mut(red).insert(Visibility::Hidden);
    oracle.ready(app, &[camera_a, camera_b])?;
    let (_, hidden) = oracle.read(app, camera_b)?;
    assert_eq!(hidden.counts[..2], [0, 0]);
    assert_rgb(hidden.axes[1], Vec3::ZERO);
    app.world_mut()
        .entity_mut(white)
        .insert(Visibility::Visible);
    app.world_mut().entity_mut(red).insert(Visibility::Visible);
    oracle.ready(app, &[camera_a, camera_b])?;
    let (_, restored) = oracle.read(app, camera_b)?;
    assert_eq!(restored.counts[..2], [2, 2]);
    assert_rgb(
        restored.axes[1],
        Vec3::splat(direction.y) + Vec3::new(2.0, 0.0, 0.0),
    );
    Ok(())
}
