//! Actual-device readback of the production vertex expansion against the CPU oracle.
//!
//! The compute entrypoint calls the exact function read from the shipped shader; it
//! does not reproduce its geometry. Native capture separately validates full material
//! imports, vertex/fragment specialization, blend state, visibility and rasterization.

use super::emission::EmitterHistory;
use super::gpu::{FrameGpu, GpuResources, Packet, PacketLayout, Publication};
use super::ribbon::{allocate_mesh, update_mesh, RibbonPose};
use super::{MistStyle, MistWakeMaterial, MistWakeUniformGpu};
use bevy::prelude::*;
use bevy::render::mesh::VertexAttributeValues;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use std::borrow::Cow;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Input {
    topology: [u32; 4],
    camera: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Output {
    position: [f32; 4],
    normal: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}

const READBACK_ENTRY: &str = r"
struct OracleInput { topology: vec4<u32>, camera: vec4<f32> };
struct OracleOutput { position: vec4<f32>, normal: vec4<f32>, uv: vec4<f32>, color: vec4<f32> };
@group(3) @binding(0) var<storage, read> inputs: array<OracleInput>;
@group(3) @binding(1) var<storage, read_write> outputs: array<OracleOutput>;
@compute @workgroup_size(64)
fn read_geometry(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&inputs) { return; }
    let input = inputs[id.x];
    let value = geometry_vertex(input.topology, input.camera.xyz);
    outputs[id.x] = OracleOutput(vec4<f32>(value.position, 1.0),
        vec4<f32>(value.normal, 0.0), vec4<f32>(value.uv, value.uv_b), value.color);
}
";

fn storage_entry(binding: u32, read_only: bool) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn read_geometry(
    device: &RenderDevice,
    queue: &RenderQueue,
    packet: &Packet,
    inputs: &[Input],
    shader: &str,
) -> Result<Vec<Output>, Box<dyn std::error::Error>> {
    let start = shader
        .find("struct EmitterHeader {")
        .ok_or("missing actual header")?;
    let end = shader
        .find("@vertex\n")
        .ok_or("missing actual vertex entry")?;
    let source = format!("{}\n{READBACK_ENTRY}", &shader[start..end]);
    let module = device.create_shader_module(ShaderModuleDescriptor {
        label: Some("actual mist geometry readback"),
        source: ShaderSource::Wgsl(Cow::Owned(source)),
    });
    let empty_layout = device.create_bind_group_layout("empty oracle layout", &[]);
    let packet_layout = device.create_bind_group_layout(
        "actual packet layout",
        &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, true),
        ],
    );
    let oracle_layout = device.create_bind_group_layout(
        "oracle readback layout",
        &[storage_entry(0, true), storage_entry(1, false)],
    );
    let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
        label: Some("actual mist geometry oracle"),
        bind_group_layouts: &[&empty_layout, &empty_layout, &packet_layout, &oracle_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&RawComputePipelineDescriptor {
        label: Some("actual mist geometry oracle"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("read_geometry"),
        compilation_options: PipelineCompilationOptions::default(),
        cache: None,
    });
    let initial_buffer = |label, contents| {
        device.create_buffer_with_data(&BufferInitDescriptor {
            label: Some(label),
            contents,
            usage: BufferUsages::STORAGE,
        })
    };
    let headers = initial_buffer(
        "oracle packet headers",
        bytemuck::cast_slice(&packet.headers),
    );
    let samples = initial_buffer(
        "oracle packet samples",
        bytemuck::cast_slice(&packet.samples),
    );
    let frame = initial_buffer("oracle packet frame", bytemuck::bytes_of(&packet.frame));
    let input = initial_buffer("oracle cameras and topology", bytemuck::cast_slice(inputs));
    let bytes = u64::try_from(inputs.len() * size_of::<Output>())?;
    let output = device.create_buffer(&BufferDescriptor {
        label: Some("actual geometry output"),
        size: bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("actual geometry readback"),
        size: bytes,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let empty = device.create_bind_group("empty oracle group", &empty_layout, &[]);
    let packet_group = device.create_bind_group(
        "actual packet group",
        &packet_layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: headers.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: samples.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 2,
                resource: frame.as_entire_binding(),
            },
        ],
    );
    let oracle_group = device.create_bind_group(
        "oracle group",
        &oracle_layout,
        &[
            BindGroupEntry {
                binding: 0,
                resource: input.as_entire_binding(),
            },
            BindGroupEntry {
                binding: 1,
                resource: output.as_entire_binding(),
            },
        ],
    );
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("mist oracle"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("mist oracle"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &*empty, &[]);
        pass.set_bind_group(1, &*empty, &[]);
        pass.set_bind_group(2, &*packet_group, &[]);
        pass.set_bind_group(3, &*oracle_group, &[]);
        pass.dispatch_workgroups(u32::try_from(inputs.len())?.div_ceil(64), 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    readback.slice(..).map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    let started = Instant::now();
    loop {
        device.poll(Maintain::Poll);
        match receiver.try_recv() {
            Ok(result) => {
                result?;
                break;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("readback channel disconnected".into())
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if started.elapsed() > Duration::from_secs(30) {
                    return Err("GPU oracle readback timed out".into());
                }
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
    let result =
        bytemuck::cast_slice::<u8, Output>(&readback.slice(..).get_mapped_range()).to_vec();
    readback.unmap();
    Ok(result)
}

fn attribute<const N: usize>(
    mesh: &Mesh,
    attribute: bevy::render::mesh::MeshVertexAttribute,
    index: usize,
) -> Result<[f32; N], Box<dyn std::error::Error>> {
    let floats: &[f32] = match mesh.attribute(attribute).ok_or("CPU attribute absent")? {
        VertexAttributeValues::Float32x2(values) => &values[index],
        VertexAttributeValues::Float32x3(values) => &values[index],
        VertexAttributeValues::Float32x4(values) => &values[index],
        _ => return Err("CPU attribute type changed".into()),
    };
    Ok(floats.try_into()?)
}

#[test]
fn actual_gpu_vertex_expansion_matches_cpu_positions_and_attributes() -> TestResult {
    let root = crate::ResolvedAssetsRoot::from_directory(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"),
    )?;
    let style = MistStyle::load(&root.as_path().join("catalog/mist.ron"))?;
    let shader = std::fs::read_to_string(root.as_path().join("shaders/mist_wake.wgsl"))?;
    let mut app = App::new();
    app.add_plugins(
        crate::presentation::GamePresentationPlugins
            .set(WindowPlugin {
                primary_window: None,
                ..default()
            })
            .disable::<bevy::winit::WinitPlugin>(),
    );
    app.finish();
    app.cleanup();
    let result = validate_geometry(
        app.world().resource::<RenderDevice>(),
        app.world().resource::<RenderQueue>(),
        &style,
        &shader,
    );
    let result = result.and_then(|()| {
        super::upload::verify_publication_and_sorting(
            app.world().resource::<RenderDevice>(),
            app.world().resource::<RenderQueue>(),
        )
    });
    crate::render_compatibility::finish_rendering(&mut app)?;
    result
}

fn validate_geometry(
    device: &RenderDevice,
    queue: &RenderQueue,
    style: &MistStyle,
    shader: &str,
) -> TestResult {
    let steps = [0u32, 2, 30, 100, 160];
    let layout = PacketLayout::new(steps.len(), style.samples_per_emitter)?;
    let mut packet = Packet::new(layout);
    packet.frame = FrameGpu {
        appearance: MistWakeUniformGpu::default(),
        geometry: Vec4::new(style.lifetime_seconds, style.width_radii, 0.0, 0.0),
    };
    let resources = GpuResources::new(
        device,
        layout,
        Arc::new(Mutex::new(Publication::new(layout))),
    )?;
    let material = MistWakeMaterial {
        headers: resources.headers,
        samples: resources.samples,
        frame: resources.frame,
    };
    assert_eq!(material.alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(material.depth_bias(), 0.0);
    let mut inputs = Vec::new();
    let mut expected = Vec::new();
    for (slot, steps) in steps.into_iter().enumerate() {
        let mut history = EmitterHistory::new(style)?;
        let radius = 0.75 + f32::from(u16::try_from(slot)?) * 0.35;
        for frame in 0..=steps {
            let t = f32::from(u16::try_from(frame)?) / 60.0;
            history.advance(
                Vec3::new(t * 3.0, (t * 0.7).sin(), (t * 1.3).cos()) + Vec3::new(91.0, 3.0, -37.0),
                radius,
                1.0 / 60.0,
                style,
            )?;
        }
        let origin = Vec3::new(94.0, 3.5, -36.0);
        let offset = Vec3::new(-2.0, 0.25, 0.5);
        packet.write_slot(
            slot,
            layout,
            &history,
            7,
            origin,
            offset,
            radius,
            slot % 2 == 0,
            style,
        )?;
        for camera in [Vec3::new(90.0, 12.0, -27.0), Vec3::new(100.0, 4.0, -43.0)] {
            let mut mesh = allocate_mesh(style.samples_per_emitter)?;
            update_mesh(
                &mut mesh,
                &history,
                &RibbonPose {
                    origin,
                    history_offset: offset,
                    camera,
                    radius,
                    boosted: slot % 2 == 0,
                },
                style,
            )?;
            for strand in 0..3 {
                for knot in 0..style.samples_per_emitter {
                    for edge in 0..2 {
                        let index = (strand * style.samples_per_emitter + knot) * 2 + edge;
                        inputs.push(Input {
                            topology: [
                                u32::try_from(knot)?,
                                u32::try_from(edge)?,
                                u32::try_from(strand)?,
                                u32::try_from(slot)?,
                            ],
                            camera: camera.extend(0.0).to_array(),
                        });
                        let uv: [f32; 2] = attribute(&mesh, Mesh::ATTRIBUTE_UV_0, index)?;
                        let uv_b: [f32; 2] = attribute(&mesh, Mesh::ATTRIBUTE_UV_1, index)?;
                        expected.push(Output {
                            position: (Vec3::from(attribute::<3>(
                                &mesh,
                                Mesh::ATTRIBUTE_POSITION,
                                index,
                            )?) + origin)
                                .extend(1.0)
                                .to_array(),
                            normal: Vec3::from(attribute::<3>(
                                &mesh,
                                Mesh::ATTRIBUTE_NORMAL,
                                index,
                            )?)
                            .extend(0.0)
                            .to_array(),
                            uv: [uv[0], uv[1], uv_b[0], uv_b[1]],
                            color: attribute(&mesh, Mesh::ATTRIBUTE_COLOR, index)?,
                        });
                    }
                }
            }
        }
    }
    let actual = read_geometry(device, queue, &packet, &inputs, shader)?;
    assert_eq!(actual.len(), expected.len());
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        if packet.headers[inputs[index].topology[3] as usize].control.y <= 1 {
            assert_eq!(actual.color[3], 0.0);
            assert!(actual.position.into_iter().all(f32::is_finite));
            continue;
        }
        for (channel, (actual, expected)) in
            bytemuck::cast_slice::<Output, f32>(std::slice::from_ref(actual))
                .iter()
                .zip(bytemuck::cast_slice::<Output, f32>(std::slice::from_ref(
                    expected,
                )))
                .enumerate()
        {
            assert!(
                (actual - expected).abs() <= 0.00003,
                "vertex {index} channel {channel}: GPU={actual} CPU={expected}"
            );
        }
    }
    Ok(())
}
