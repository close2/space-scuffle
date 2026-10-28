//! Executes the shared-layout fixture on the real Vulkan device and checks every output scalar.
use std::{error::Error, sync::mpsc};
use wgpu::util::DeviceExt;
/// Executes the shared-layout compute workload and checks its exact scalar output.
pub fn verify() -> Result<(), Box<dyn Error>> {
    pollster::block_on(run())
}
async fn run() -> Result<(), Box<dyn Error>> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        flags: wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .ok_or("no Vulkan adapter")?;
    println!("GPU readback adapter: {:?}", adapter.get_info());
    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: Some("layout regression"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        )
        .await?;
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("real shared-layout regression"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../fixtures/shared_composite.wgsl").into()),
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("mixed interface/workgroup"),
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let values: Vec<f32> = (0..48).map(|value| value as f32).collect();
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let storage = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("four payloads"),
        contents: &bytes,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("layout output"),
        size: bytes.len() as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: storage.as_entire_binding(),
        }],
    });
    let mut commands = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    {
        let mut pass = commands.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: None,
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.dispatch_workgroups(1, 1, 1);
    }
    commands.copy_buffer_to_buffer(&storage, 0, &readback, 0, bytes.len() as u64);
    queue.submit([commands.finish()]);
    let slice = readback.slice(..);
    let (sender, receiver) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::Maintain::Wait);
    receiver.recv()??;
    let mapped = slice.get_mapped_range();
    for (index, bytes) in mapped.chunks_exact(4).enumerate() {
        let actual = f32::from_le_bytes(bytes.try_into()?);
        let payload = index / 12;
        let member = index % 12;
        let expected =
            values[((payload + 1) % 4) * 12 + member] + if member < 4 { 1.0 } else { 0.0 };
        if actual.to_bits() != expected.to_bits() {
            return Err(format!("GPU scalar {index}: expected {expected}, got {actual}").into());
        }
    }
    drop(mapped);
    readback.unmap();
    if let Some(error) = device.pop_error_scope().await {
        return Err(error.into());
    }
    println!("GPU readback: all 48 scalars match the mixed-layout transformation exactly");
    Ok(())
}
