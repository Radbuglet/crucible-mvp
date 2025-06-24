use std::{env, fs, sync::Arc};

use anyhow::Context;
use crucible_renderer::{GfxContext, TEXTURE_FORMAT};
use futures::executor::block_on;
use glam::{Affine2, U8Vec3, UVec2, Vec2};
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    runtime::{
        base::{RtModule, RtState},
        log::RtLogger,
    },
    utils::winit::{FallibleApplicationHandler, run_app_fallible},
};

#[derive(Debug)]
struct App {
    engine: wasmtime::Engine,
    linker: wasmtime::Linker<RtState>,
    wgpu_instance: wgpu::Instance,

    current_game: Option<ActiveGameState>,
    gfx_state: Option<ActiveGfxState>,

    error: Option<anyhow::Error>,
}

#[derive(Debug)]
struct ActiveGameState {
    module: wasmtime::Module,
    store: wasmtime::Store<RtState>,
    instance: wasmtime::Instance,
}

#[derive(Debug)]
struct ActiveGfxState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    context: GfxContext,
}

impl FallibleApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        block_on(async {
            if self.gfx_state.is_some() {
                return Ok(());
            }

            let window = Arc::new(
                event_loop.create_window(
                    WindowAttributes::default()
                        .with_title("Crucible")
                        .with_visible(false),
                )?,
            );

            let surface = self
                .wgpu_instance
                .create_surface(window.clone())
                .context("failed to create main surface")?;

            let adapter = self
                .wgpu_instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    force_fallback_adapter: false,
                    compatible_surface: Some(&surface),
                })
                .await?;

            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor {
                    label: None,
                    required_features: crucible_renderer::required_features(),
                    required_limits: wgpu::Limits {
                        max_binding_array_elements_per_shader_stage: 32,
                        ..wgpu::Limits::default()
                    },
                    memory_hints: wgpu::MemoryHints::default(),
                    trace: wgpu::Trace::Off,
                })
                .await?;

            let context = GfxContext::new(device.clone());

            window.set_visible(true);

            self.gfx_state = Some(ActiveGfxState {
                window,
                surface,
                adapter,
                device,
                queue,
                context,
            });

            Ok(())
        })
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()> {
        let Some(gfx_state) = &mut self.gfx_state else {
            return Ok(());
        };

        match &event {
            WindowEvent::RedrawRequested => {
                let window_size = gfx_state.window.inner_size();

                gfx_state.surface.configure(
                    &gfx_state.device,
                    &wgpu::SurfaceConfiguration {
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        format: TEXTURE_FORMAT,
                        width: window_size.width,
                        height: window_size.height,
                        present_mode: wgpu::PresentMode::default(),
                        desired_maximum_frame_latency: 2,
                        alpha_mode: wgpu::CompositeAlphaMode::default(),
                        view_formats: Vec::new(),
                    },
                );

                let texture = gfx_state.surface.get_current_texture()?;

                gfx_state.context.clear_texture(
                    &texture.texture,
                    wgpu::Color {
                        r: 0.,
                        g: 1.,
                        b: 0.4,
                        a: 1.0,
                    },
                );

                gfx_state.context.draw_texture(
                    &texture.texture,
                    None,
                    Affine2::from_scale_angle_translation(
                        Vec2::new(0.1, 0.1),
                        (10f32).to_radians(),
                        Vec2::ZERO,
                    ),
                    (UVec2::ZERO, UVec2::new(1, 1)),
                    U8Vec3::new(34, 45, 100).extend(0xFF),
                )?;

                gfx_state.context.submit(&gfx_state.queue);
                texture.present();
            }
            _ => {}
        }

        Ok(())
    }
}

pub fn run_app() -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    let engine = wasmtime::Engine::new(&wasmtime::Config::default())?;
    let gfx_instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        backend_options: wgpu::BackendOptions::default(),
    });

    let module_path = env::args().nth(1).context("no module supplied")?;
    let module = fs::read(&module_path)
        .with_context(|| format!("failed to read module at `{module_path}`"))?;

    let mut linker = wasmtime::Linker::new(&engine);
    RtLogger::define(&mut linker)?;

    let module = wasmtime::Module::new(&engine, module)?;
    let mut store = wasmtime::Store::new(&engine, RtState::default());
    let instance = linker.instantiate(&mut store, &module)?;

    let mut app = App {
        engine,
        linker,
        current_game: Some(ActiveGameState {
            module,
            store,
            instance,
        }),
        wgpu_instance: gfx_instance,
        gfx_state: None,
        error: None,
    };

    run_app_fallible(event_loop, &mut app)?;

    if let Some(e) = app.error.take() {
        return Err(e);
    }

    Ok(())
}
