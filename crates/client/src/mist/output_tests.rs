//! Real-device checks of the shipped mist output function and Bevy shader imports.
//!
//! Compute readback isolates color/alpha arithmetic without replacing the engine's
//! tonemapping implementation. Native HDR/LDR captures separately verify full material
//! specialization, attachments, blending and the postprocess path.

use bevy::prelude::*;
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderDevice, RenderQueue};
use bevy::render::RenderApp;
use std::time::{Duration, Instant};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const IMPORTS: &str = r"
#import bevy_render::view::ColorGrading
#ifdef TONEMAP_IN_SHADER
#import bevy_core_pipeline::tonemapping::{tone_mapping, screen_space_dither}
#import bevy_render::maths::powsafe
#endif
";
const ENTRY: &str = r"
@group(0) @binding(0) var<storage, read> inputs: array<vec4<f32>>;
@group(0) @binding(1) var<storage, read_write> outputs: array<vec4<f32>>;
@compute @workgroup_size(1)
fn read_output(@builtin(global_invocation_id) id: vec3<u32>) {
    var grading: ColorGrading;
    grading.exposure = 1.0;
    grading.post_saturation = 1.0;
    let input = inputs[id.x];
    outputs[id.x] = mist_output(input.rgb, input.a, vec2<f32>(17.5, 31.5), grading);
}
";

#[test]
fn actual_gpu_output_preserves_alpha_and_tonemaps_before_premultiplication() -> TestResult {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/shaders/mist_wake.wgsl");
    let shader = std::fs::read_to_string(path)?;
    let begin = shader
        .find("fn mist_output(")
        .ok_or("missing production output function")?;
    let end = shader[begin..]
        .find("@fragment")
        .ok_or("missing fragment entrypoint")?
        + begin;
    let source = format!("{IMPORTS}\n{}\n{ENTRY}", &shader[begin..end]);
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
    let result = verify_variants(&mut app, source);
    crate::render_compatibility::finish_rendering(&mut app)?;
    result
}

fn verify_variants(app: &mut App, source: String) -> TestResult {
    let device = app.world().resource::<RenderDevice>().clone();
    let queue = app.world().resource::<RenderQueue>().clone();
    let layout =
        device.create_bind_group_layout("mist output readback", &[entry(0, true), entry(1, false)]);
    let shader = app
        .world_mut()
        .resource_mut::<Assets<Shader>>()
        .add(Shader::from_wgsl(
            source,
            "shipped-mist-output-readback.wgsl",
        ));
    // Include HDR-range radiance, multiple optical thicknesses, and exact zero alpha.
    let inputs: Vec<[f32; 4]> = [0.0, 0.01, 0.25, 0.75, 1.0]
        .into_iter()
        .flat_map(|alpha| [[0.1, 0.7, 4.0, alpha], [0.0, 0.0, 0.0, alpha]])
        .collect();
    for (tonemap, dither) in [(false, false), (true, false), (true, true)] {
        let mut defs = vec![
            ShaderDefVal::UInt(
                "TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(),
                bevy::pbr::TONEMAPPING_LUT_TEXTURE_BINDING_INDEX,
            ),
            ShaderDefVal::UInt(
                "TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(),
                bevy::pbr::TONEMAPPING_LUT_SAMPLER_BINDING_INDEX,
            ),
        ];
        if tonemap {
            defs.extend(["TONEMAP_IN_SHADER".into(), "TONEMAP_METHOD_REINHARD".into()]);
        }
        if dither {
            defs.push("DEBAND_DITHER".into());
        }
        let id = app
            .sub_app(RenderApp)
            .world()
            .resource::<PipelineCache>()
            .queue_compute_pipeline(ComputePipelineDescriptor {
                label: Some("actual mist output transform".into()),
                layout: vec![layout.clone()],
                push_constant_ranges: vec![],
                shader: shader.clone(),
                shader_defs: defs,
                entry_point: "read_output".into(),
                zero_initialize_workgroup_memory: true,
            });
        let started = Instant::now();
        let pipeline = loop {
            app.update();
            let cache = app.sub_app(RenderApp).world().resource::<PipelineCache>();
            if let Some(pipeline) = cache.get_compute_pipeline(id) {
                break pipeline.clone();
            }
            if let CachedPipelineState::Err(error) = cache.get_compute_pipeline_state(id) {
                if !matches!(
                    error,
                    PipelineCacheError::ShaderNotLoaded(_)
                        | PipelineCacheError::ShaderImportNotYetAvailable
                ) {
                    return Err(format!("actual output shader failed: {error}").into());
                }
            }
            if started.elapsed() > Duration::from_secs(30) {
                return Err("output shader readiness timeout".into());
            }
            std::thread::yield_now();
        };
        let values = dispatch(&device, &queue, &layout, &pipeline, &inputs)?;
        for (input, output) in inputs.iter().zip(values) {
            assert!(output.iter().all(|v| v.is_finite()));
            assert_eq!(output[3], input[3]);
            for channel in 0..3 {
                let mut expected = input[channel];
                if tonemap {
                    expected = (expected * 2.0) / (1.0 + expected * 2.0);
                    if dither {
                        let divisor = [103.0_f32, 71.0, 97.0][channel];
                        let noise = ((171.0_f32 * 17.5 + 231.0 * 31.5) / divisor).fract();
                        let perceptual = expected.powf(1.0 / 2.2) + (noise - 0.5) / 255.0;
                        expected = perceptual.abs().powf(2.2) * perceptual.signum();
                    }
                }
                expected *= input[3];
                assert!((output[channel] - expected).abs() < 0.00002,
                    "tonemap={tonemap} dither={dither} input={input:?} output={output:?} expected={expected}");
            }
            if input[3] == 0.0 {
                assert_eq!(output, [0.0; 4]);
            }
        }
    }
    Ok(())
}

fn entry(binding: u32, read_only: bool) -> BindGroupLayoutEntry {
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

fn dispatch(
    device: &RenderDevice,
    queue: &RenderQueue,
    layout: &BindGroupLayout,
    pipeline: &ComputePipeline,
    inputs: &[[f32; 4]],
) -> Result<Vec<[f32; 4]>, Box<dyn std::error::Error>> {
    let input = device.create_buffer_with_data(&BufferInitDescriptor {
        label: Some("mist straight linear samples"),
        contents: bytemuck::cast_slice(inputs),
        usage: BufferUsages::STORAGE,
    });
    let bytes = u64::try_from(size_of_val(inputs))?;
    let output = device.create_buffer(&BufferDescriptor {
        label: Some("mist premultiplied samples"),
        size: bytes,
        usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("mist output readback"),
        size: bytes,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(
        "mist output samples",
        layout,
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
        label: Some("mist color oracle"),
    });
    {
        let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
            label: Some("mist color oracle"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &*group, &[]);
        pass.dispatch_workgroups(u32::try_from(inputs.len())?, 1, 1);
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
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(error) => return Err(error.into()),
        }
        if started.elapsed() > Duration::from_secs(30) {
            return Err("output GPU readback timeout".into());
        }
        std::thread::yield_now();
    }
    let values = bytemuck::cast_slice(&readback.slice(..).get_mapped_range()).to_vec();
    readback.unmap();
    Ok(values)
}
