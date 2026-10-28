//! Real native-surface comparison of unrendered presentation and initialized presentation.
use std::{error::Error, sync::Arc};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
struct Graphics {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
}
#[derive(Default)]
struct Probe {
    graphics: Option<Graphics>,
    clear: bool,
    frames: u32,
    error: Option<String>,
}
impl Probe {
    fn initialize(event_loop: &ActiveEventLoop) -> Result<Graphics, Box<dyn Error>> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Space Scuffle surface compatibility probe")
                    .with_inner_size(winit::dpi::PhysicalSize::new(640, 480)),
            )?,
        );
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::DEBUG | wgpu::InstanceFlags::VALIDATION,
            ..Default::default()
        });
        let surface = instance.create_surface(window.clone())?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(&surface),
            ..Default::default()
        }))
        .ok_or("no native Vulkan surface adapter")?;
        println!("surface adapter: {:?}", adapter.get_info());
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None))?;
        let config = surface
            .get_default_config(&adapter, 640, 480)
            .ok_or("surface configuration unavailable")?;
        surface.configure(&device, &config);
        Ok(Graphics {
            window,
            surface,
            device,
            queue,
        })
    }
    fn render(&self) -> Result<(), Box<dyn Error>> {
        let graphics = self.graphics.as_ref().ok_or("graphics not initialized")?;
        let texture = graphics.surface.get_current_texture()?;
        if self.clear {
            let view = texture
                .texture
                .create_view(&wgpu::TextureViewDescriptor::default());
            let mut encoder = graphics
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("initialize actual acquired surface"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }
            graphics.queue.submit([encoder.finish()]);
        }
        texture.present();
        Ok(())
    }
}
impl ApplicationHandler for Probe {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match Self::initialize(event_loop) {
            Ok(graphics) => {
                graphics.window.request_redraw();
                self.graphics = Some(graphics);
            }
            Err(error) => {
                self.error = Some(error.to_string());
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
            return;
        }
        if !matches!(event, WindowEvent::RedrawRequested) {
            return;
        }
        if let Err(error) = self.render() {
            self.error = Some(error.to_string());
            event_loop.exit();
            return;
        }
        self.frames += 1;
        if self.frames == 6 {
            event_loop.exit();
        } else if let Some(graphics) = &self.graphics {
            graphics.window.request_redraw();
        }
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let mut probe = Probe {
        clear: std::env::args().any(|arg| arg == "--clear"),
        ..Default::default()
    };
    EventLoop::new()?.run_app(&mut probe)?;
    if let Some(error) = probe.error {
        return Err(error.into());
    }
    println!(
        "presented {} real native frames, initialized={}",
        probe.frames, probe.clear
    );
    Ok(())
}
