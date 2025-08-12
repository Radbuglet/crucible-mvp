use std::sync::Arc;

use anyhow::Context as _;
use arid::{Destructor, Handle, Strong, W, Wr};
use arid_entity::{ComponentHandle, EntityHandle, component, component_internals::Object};
use crucible_renderer::{Renderer, TEXTURE_FORMAT};
use rustc_hash::FxHashMap;
use winit::window::{Window, WindowId};

use crate::utils::winit::is_in_live_resize;

// === GfxContext === //

pub type GfxContext = Arc<GfxContextInner>;

#[derive(Debug)]
pub struct GfxContextInner {
    pub adapter: wgpu::Adapter,
    pub instance: wgpu::Instance,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub features: wgpu::Features,
    pub limits: wgpu::Limits,
}

pub async fn create_gfx_context(
    compatible_window: Arc<Window>,
) -> anyhow::Result<(GfxContext, wgpu::Surface<'static>)> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::default(),
        backend_options: wgpu::BackendOptions::default(),
    });

    let surface = instance
        .create_surface(compatible_window)
        .context("failed to create main surface")?;

    let adapter = instance
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

    Ok((
        Arc::new(GfxContextInner {
            adapter,
            instance,
            queue,
            features: device.features(),
            limits: device.limits(),
            device,
        }),
        surface,
    ))
}

// === WindowManager === //

#[derive(Debug)]
pub struct WindowManager {
    gfx: GfxContext,
    windows: FxHashMap<WindowId, WindowStateHandle>,
}

component!(pub WindowManager);

impl WindowManagerHandle {
    pub fn new(gfx: GfxContext, w: W) -> Strong<Self> {
        WindowManager {
            gfx,
            windows: FxHashMap::default(),
        }
        .spawn(w)
    }

    pub fn gfx(self, w: Wr<'_>) -> &GfxContext {
        &self.r(w).gfx
    }

    pub fn create_window(self, window: Arc<Window>, surface: wgpu::Surface<'static>, w: W) {
        let window_id = window.id();
        let window_state = WindowState {
            manager: self,
            window,
            surface,
            renderer: Renderer::new(self.r(w).gfx.device.clone()),
        }
        .spawn(w);

        let window_entity = EntityHandle::new(w)
            .with_label("window", w)
            .with(window_state.clone(), w);

        self.entity(w).with_child(window_entity, w);

        self.m(w).windows.insert(window_id, window_state.as_weak());
    }

    pub fn lookup(self, id: WindowId, w: Wr) -> WindowStateHandle {
        self.r(w).windows[&id]
    }
}

// === WindowState === //

#[derive(Debug)]
pub struct WindowState {
    manager: WindowManagerHandle,
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    renderer: Renderer,
}

component!(pub WindowState);

impl WindowStateHandle {
    pub fn window(self, w: Wr<'_>) -> &Arc<Window> {
        &self.r(w).window
    }

    pub fn renderer(self, w: W) -> &Renderer {
        &self.m(w).renderer
    }

    pub fn renderer_mut(self, w: W) -> &mut Renderer {
        &mut self.m(w).renderer
    }

    pub fn is_in_live_resize(self, w: Wr) -> bool {
        is_in_live_resize(self.window(w))
    }

    pub fn redraw(
        self,
        f: impl FnOnce(&wgpu::SurfaceTexture, W) -> anyhow::Result<()>,
        w: W,
    ) -> anyhow::Result<()> {
        let gfx = self.r(w).manager.r(w).gfx.clone();
        let window_size = self.window(w).inner_size();

        self.r(w).surface.configure(
            &gfx.device,
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

        let texture = self.r(w).surface.get_current_texture()?;

        f(&texture, w)?;

        self.m(w).renderer.submit(&gfx.queue);
        texture.present();

        Ok(())
    }
}

impl Destructor for WindowStateHandle {
    fn pre_destroy(self, w: W) {
        let window_id = self.r(w).window.id();
        self.r(w).manager.m(w).windows.remove(&window_id);
    }
}
