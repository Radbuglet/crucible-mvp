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
    renderer: Renderer,
    windows: FxHashMap<WindowId, WindowStateHandle>,
}

component!(pub WindowManager);

impl WindowManagerHandle {
    pub fn new(gfx: GfxContext, w: W) -> Strong<Self> {
        let renderer = Renderer::new(gfx.device.clone());

        WindowManager {
            gfx,
            renderer,
            windows: FxHashMap::default(),
        }
        .spawn(w)
    }

    pub fn gfx(self, w: Wr<'_>) -> &GfxContext {
        &self.r(w).gfx
    }

    pub fn renderer(self, w: Wr<'_>) -> &Renderer {
        &self.r(w).renderer
    }

    pub fn renderer_mut(self, w: W<'_>) -> &mut Renderer {
        &mut self.m(w).renderer
    }

    pub fn create_window(
        self,
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        w: W,
    ) -> WindowStateHandle {
        let window_id = window.id();
        let window_state = WindowState {
            manager: self,
            window,
            surface,
            surface_texture: None,
        }
        .spawn(w);

        let window_entity = EntityHandle::new(w)
            .with_label("window", w)
            .with(window_state.clone(), w);

        self.entity(w).with_child(window_entity, w);

        self.m(w).windows.insert(window_id, window_state.as_weak());

        window_state.as_weak()
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
    surface_texture: Option<wgpu::SurfaceTexture>,
}

component!(pub WindowState);

impl WindowStateHandle {
    pub fn window(self, w: Wr<'_>) -> &Arc<Window> {
        &self.r(w).window
    }

    pub fn is_in_live_resize(self, w: Wr) -> bool {
        is_in_live_resize(self.window(w))
    }

    pub fn start_redraw(self, w: W) -> anyhow::Result<Option<wgpu::Texture>> {
        if self.r(w).surface_texture.is_some() {
            return Ok(None);
        }

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

        let fb = self.r(w).surface.get_current_texture()?;
        let texture = fb.texture.clone();
        self.m(w).surface_texture = Some(fb);

        Ok(Some(texture))
    }

    pub fn end_redraw(self, w: W) {
        let fb = self.m(w).surface_texture.take().unwrap();
        let gfx = self.r(w).manager.r(w).gfx.clone();

        self.r(w).manager.m(w).renderer.submit(&gfx.queue);

        fb.present();
    }
}

impl Destructor for WindowStateHandle {
    fn pre_destroy(self, w: W) {
        let window_id = self.r(w).window.id();
        self.r(w).manager.m(w).windows.remove(&window_id);
    }
}
