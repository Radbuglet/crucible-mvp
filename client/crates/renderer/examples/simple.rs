use std::{fs::File, io::BufReader};

use crucible_renderer::{GfxContext, REQUIRED_FEATURES, TEXTURE_FORMAT};
use futures::executor::block_on;
use glam::{Affine2, U8Vec4, UVec2, Vec2};
use image::ImageFormat;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{WindowAttributes, WindowId},
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
    surface: wgpu::Surface<'static>,
    image: wgpu::Texture,
    gfx: GfxContext,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.0.get_or_insert_with(|| {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
                backends: wgpu::Backends::PRIMARY,
                flags: wgpu::InstanceFlags::empty(),
                backend_options: wgpu::BackendOptions::default(),
            });

            let surface = event_loop
                .create_window(WindowAttributes::default().with_title("Crucible"))
                .unwrap();

            let surface = instance.create_surface(surface).unwrap();

            let (device, queue) = block_on(async {
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
                        required_features: REQUIRED_FEATURES,
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

            let mut gfx = GfxContext::new(device.clone());

            let image_cpu = image::load(
                BufReader::new(File::open("demo.png").unwrap()),
                ImageFormat::Png,
            )
            .unwrap()
            .into_rgba8();

            let image_cpu_bgra = bytemuck::cast_slice::<u8, [u8; 4]>(image_cpu.as_raw())
                .iter()
                .copied()
                .map(|[r, g, b, a]| [b, g, r, a])
                .collect::<Vec<[u8; 4]>>();

            let image = gfx.create_texture(image_cpu.width(), image_cpu.height());

            gfx.upload_texture(
                &image,
                &image_cpu_bgra,
                UVec2::new(image_cpu.width(), image_cpu.height()),
                UVec2::ZERO,
                None,
            )
            .unwrap();

            AppState {
                device,
                queue,
                surface,
                image,
                gfx,
            }
        });
    }

    #[allow(clippy::single_match)]
    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(app) = &mut self.0 else {
            return;
        };

        match event {
            WindowEvent::RedrawRequested => {
                app.surface.configure(
                    &app.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: TEXTURE_FORMAT,
                        width: 1920,
                        height: 1080,
                        present_mode: wgpu::PresentMode::default(),
                        desired_maximum_frame_latency: 2,
                        alpha_mode: wgpu::CompositeAlphaMode::default(),
                        view_formats: Vec::new(),
                    },
                );

                let texture = app.surface.get_current_texture().unwrap();

                app.gfx
                    .draw_texture(
                        &texture.texture,
                        Some(&app.image),
                        Affine2::from_scale_angle_translation(
                            Vec2::new(0.1, -0.1),
                            0.,
                            Vec2::new(0.1, 0.0),
                        ),
                        (
                            UVec2::ZERO,
                            UVec2::new(app.image.width(), app.image.height()),
                        ),
                        U8Vec4::new(255, 50, 100, 255),
                    )
                    .unwrap();

                app.gfx.submit(&app.queue);

                texture.present();
            }
            _ => {}
        }
    }
}
