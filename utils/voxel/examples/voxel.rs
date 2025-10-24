use std::sync::Arc;

use crucible_voxel::{driver::VoxelRenderer, utils::AssetManager};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

fn main() {
    EventLoop::new().unwrap().run_app(&mut App(None)).unwrap();
}

#[derive(Debug)]
struct App(Option<AppState>);

#[derive(Debug)]
struct AppState {
    device: wgpu::Device,
    queue: wgpu::Queue,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    renderer: VoxelRenderer,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.0.get_or_insert_with(|| {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::PRIMARY,
                flags: wgpu::InstanceFlags::empty(),
                backend_options: wgpu::BackendOptions::default(),
                memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            });

            let window = event_loop
                .create_window(WindowAttributes::default().with_title("Crucible"))
                .unwrap();

            let window = Arc::new(window);

            let surface = instance.create_surface(window.clone()).unwrap();

            let (device, queue) = async_io::block_on(async {
                let adapter = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::None,
                        force_fallback_adapter: false,
                        compatible_surface: Some(&surface),
                    })
                    .await
                    .unwrap();

                adapter
                    .request_device(&wgpu::DeviceDescriptor {
                        label: None,
                        required_features: wgpu::Features::default(),
                        required_limits: wgpu::Limits {
                            max_binding_array_elements_per_shader_stage: 32,
                            ..Default::default()
                        },
                        memory_hints: wgpu::MemoryHints::Performance,
                        trace: wgpu::Trace::Off,
                    })
                    .await
                    .unwrap()
            });

            let renderer = VoxelRenderer::new(device.clone(), AssetManager::default());

            AppState {
                device,
                queue,
                window,
                surface,
                renderer,
            }
        });
    }

    #[allow(clippy::single_match)]
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = &mut self.0 else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                let window_size = app.window.inner_size();

                app.surface.configure(
                    &app.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: wgpu::TextureFormat::Bgra8Unorm,
                        width: window_size.width,
                        height: window_size.height,
                        present_mode: wgpu::PresentMode::default(),
                        desired_maximum_frame_latency: 2,
                        alpha_mode: wgpu::CompositeAlphaMode::default(),
                        view_formats: Vec::new(),
                    },
                );

                let surface = app.surface.get_current_texture().unwrap();

                let surface_view = surface
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut encoder = app
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

                app.renderer.submit(&mut encoder, &surface_view);

                app.queue.submit([encoder.finish()]);

                surface.present();
            }
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => {}
        }
    }
}
