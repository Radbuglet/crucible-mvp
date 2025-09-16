use std::mem;

use arid::{Handle, MayDangle, Strong, W, Wr};
use arid_entity::{Component, EntityHandle, component};
use crucible_abi as abi;
use glam::Affine2;
use wasmlink::HostClosure;
use wasmlink_wasmtime::{WslLinker, WslLinkerExt};

use crate::{
    services::window::{WindowManagerHandle, WindowStateHandle},
    utils::arena::GuestArena,
};

#[derive(Debug)]
pub struct GfxBindings {
    window_mgr: WindowManagerHandle,
    handles: GuestArena<GfxTexture>,
    user_callbacks: Option<WindowCallbacks>,
    redraw_requested: bool,
}

#[derive(Debug)]
struct GfxTexture {
    wgpu: wgpu::Texture,
    fb_owned_by: Option<MayDangle<WindowStateHandle>>,
}

#[derive(Debug, Copy, Clone)]
pub struct WindowCallbacks {
    pub redraw_requested: HostClosure<abi::RedrawRequestedArgs>,
    pub mouse_event: HostClosure<abi::MouseEvent>,
    pub mouse_moved: HostClosure<abi::DVec2>,
    pub key_event: HostClosure<abi::KeyEvent>,
    pub exit_requested: HostClosure<()>,
}

component!(pub GfxBindings);

impl GfxBindingsHandle {
    pub fn new(owner: EntityHandle, window_mgr: WindowManagerHandle, w: W) -> Strong<Self> {
        GfxBindings {
            window_mgr,
            handles: GuestArena::default(),
            user_callbacks: None,
            redraw_requested: false,
        }
        .attach(owner, w)
    }

    pub fn user_callbacks(self, w: Wr) -> Option<WindowCallbacks> {
        self.r(w).user_callbacks
    }

    pub fn create_texture(
        self,
        texture: wgpu::Texture,
        fb_owned_by: Option<WindowStateHandle>,
        w: W,
    ) -> anyhow::Result<u32> {
        self.m(w).handles.add(GfxTexture {
            wgpu: texture,
            fb_owned_by: fb_owned_by.map(MayDangle::new),
        })
    }

    #[must_use]
    pub fn take_redraw_request(self, w: W) -> bool {
        mem::take(&mut self.m(w).redraw_requested)
    }

    pub fn install(self, linker: &mut WslLinker) -> anyhow::Result<()> {
        linker.define_wsl(abi::WINDOW_BIND_HANDLERS, move |cx, args, ret| {
            let w = cx.w();

            self.m(w).user_callbacks = Some(WindowCallbacks {
                redraw_requested: args.redraw_requested,
                mouse_event: args.mouse_event,
                mouse_moved: args.mouse_moved,
                key_event: args.key_event,
                exit_requested: args.exit_requested,
            });

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::WINDOW_UNBIND_HANDLERS, move |cx, (), ret| {
            let w = cx.w();

            self.m(w).user_callbacks = None;

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::WINDOW_REQUEST_REDRAW, move |cx, (), ret| {
            self.m(cx.w()).redraw_requested = true;

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::GPU_CREATE_TEXTURE, move |cx, size, ret| {
            let w = cx.w();

            // TODO: limit size

            let texture = self
                .m(w)
                .window_mgr
                .renderer_mut(w)
                .create_texture(size.x, size.y);

            let handle = self.create_texture(texture, None, w)?;

            ret.finish(cx, &abi::GpuTextureHandle { raw: handle })
        })?;

        linker.define_wsl(abi::GPU_CLEAR_TEXTURE, move |cx, args, ret| {
            let w = cx.w();

            let texture = self.r(w).handles.get(args.handle.raw)?.wgpu.clone();

            self.r(w).window_mgr.renderer_mut(w).clear_texture(
                &texture,
                wgpu::Color {
                    r: args.color.r as f64 / 255.0,
                    g: args.color.g as f64 / 255.0,
                    b: args.color.b as f64 / 255.0,
                    a: args.color.a as f64 / 255.0,
                },
            );

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::GPU_UPLOAD_TEXTURE, move |cx, args, ret| {
            let (w, mem) = cx.world_and_memory();

            let texture = self.r(w).handles.get(args.handle.raw)?.wgpu.clone();
            let buffer = bytemuck::cast_slice(args.buffer.slice().read(mem)?).to_vec();

            self.r(w).window_mgr.renderer_mut(w).upload_texture(
                &texture,
                &buffer,
                bytemuck::cast(args.buffer_size),
                bytemuck::cast(args.at),
                args.clip
                    .map(|v| (bytemuck::cast(v.origin), bytemuck::cast(v.size))),
            )?;

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::GPU_DRAW_TEXTURE, move |cx, args, ret| {
            let w = cx.w();

            let src_texture = match args.src_handle {
                Some(v) => Some(self.r(w).handles.get(v.raw)?.wgpu.clone()),
                None => None,
            };

            let dst_texture = self.r(w).handles.get(args.dst_handle.raw)?.wgpu.clone();

            self.r(w).window_mgr.renderer_mut(w).draw_texture(
                &dst_texture,
                src_texture.as_ref(),
                Affine2::from_cols_array(&args.transform.comps),
                args.clip
                    .map(|rect| (bytemuck::cast(rect.origin), bytemuck::cast(rect.size))),
                bytemuck::cast(args.tint),
            )?;

            ret.finish(cx, &())
        })?;

        linker.define_wsl(abi::GPU_DESTROY_TEXTURE, move |cx, args, ret| {
            let w = cx.w();
            let texture = self.m(w).handles.remove(args.raw)?;

            if let Some(fb_owned_by) = texture.fb_owned_by
                && let Some(fb_owned_by) = fb_owned_by.get(w)
            {
                fb_owned_by.end_redraw(w);
            }

            ret.finish(cx, &())
        })?;

        Ok(())
    }
}
