use std::{cell::RefCell, rc::Rc};

use anyhow::Context;
use crucible_renderer::GfxContext;
use crucible_shared::utils::wasm::{
    MainMemory, MemoryExt as _, RtFieldExt, RtModule, RtState, RtStateNs,
};
use glam::{Affine2, U8Vec4, UVec2};
use late_struct::late_field;

use crate::utils::arena::Arena;

#[derive(Debug)]
pub struct RtRenderer {
    textures: Arena<wgpu::Texture>,
    context: Rc<RefCell<GfxContext>>,
    swapchain_texture: Option<wgpu::Texture>,
}

late_field!(RtRenderer[RtStateNs] => Option<RtRenderer>);

impl RtRenderer {
    pub fn init(
        store: &mut wasmtime::Store<RtState>,
        context: Rc<RefCell<GfxContext>>,
    ) -> anyhow::Result<()> {
        *Self::get_mut(store) = Some(Self {
            textures: Arena::default(),
            context,
            swapchain_texture: None,
        });

        Ok(())
    }

    pub fn set_swapchain_texture(&mut self, texture: Option<wgpu::Texture>) {
        self.swapchain_texture = texture;
    }
}

impl RtModule for RtRenderer {
    fn define(linker: &mut wasmtime::Linker<RtState>) -> anyhow::Result<()> {
        linker.func_wrap(
            "crucible",
            "create_texture",
            |mut caller: wasmtime::Caller<'_, RtState>,
             width: u32,
             height: u32|
             -> anyhow::Result<u32> {
                let Some(renderer) = Self::get_mut(&mut caller) else {
                    anyhow::bail!("no renderer initialized");
                };

                let texture = renderer.context.borrow_mut().create_texture(width, height);
                let handle = renderer.textures.add(texture)?;

                Ok(handle)
            },
        )?;

        linker.func_wrap(
            "crucible",
            "get_swapchain_texture",
            |mut caller: wasmtime::Caller<'_, RtState>, out_size: u32| -> anyhow::Result<u32> {
                let (mem, state) = MainMemory::data_state_mut(&mut caller);

                let Some(renderer) = state.get_mut::<Self>() else {
                    anyhow::bail!("no renderer initialized");
                };

                let texture = renderer
                    .swapchain_texture
                    .clone()
                    .context("no swapchain actively bound")?;

                *mem.mem_arr_mut(out_size)? = [texture.width().to_le(), texture.height().to_le()];

                let handle = renderer.textures.add(texture)?;

                Ok(handle)
            },
        )?;

        linker.func_wrap(
            "crucible",
            "clear_texture",
            |mut caller: wasmtime::Caller<'_, RtState>,
             handle: u32,
             color: u32|
             -> anyhow::Result<()> {
                let Some(renderer) = Self::get_mut(&mut caller) else {
                    anyhow::bail!("no renderer initialized");
                };

                let [r, g, b, a] = color.to_le_bytes();

                let texture = renderer.textures.get(handle)?;

                renderer.context.borrow_mut().clear_texture(
                    texture,
                    wgpu::Color {
                        r: r as f64 / 255.,
                        g: g as f64 / 255.,
                        b: b as f64 / 255.,
                        a: a as f64 / 255.,
                    },
                );

                Ok(())
            },
        )?;

        linker.func_wrap(
            "crucible",
            "upload_texture",
            |mut caller: wasmtime::Caller<'_, RtState>,
             target_id: u32,
             buffer: u32, // *const Color8,
             buffer_width: u32,
             buffer_height: u32,
             at_x: u32,
             at_y: u32,
             // clip: *const [u32; 4]
             clip: u32|
             -> anyhow::Result<()> {
                let (mem, state) = MainMemory::data_state_mut(&mut caller);

                let Some(renderer) = state.get_mut::<Self>() else {
                    anyhow::bail!("no renderer initialized");
                };

                let target = renderer.textures.get(target_id)?;

                let data = mem.mem_elem::<[u8; 4]>(
                    buffer,
                    buffer_width
                        .checked_mul(buffer_height)
                        .context("buffer size too large")?,
                )?;

                let clip = if clip != 0 {
                    let [x, y, w, h] = mem.mem_arr::<u32, 4>(clip)?.map(u32::from_le);

                    Some((UVec2::new(x, y), UVec2::new(w, h)))
                } else {
                    None
                };

                renderer.context.borrow_mut().upload_texture(
                    target,
                    data,
                    UVec2::new(buffer_width, buffer_height),
                    UVec2::new(at_x, at_y),
                    clip,
                )?;

                Ok(())
            },
        )?;

        linker.func_wrap(
            "crucible",
            "draw_texture",
            |mut caller: wasmtime::Caller<'_, RtState>,
             target_id: u32,
             src_id: u32,
             transform: u32, // *const [f32; 6],
             clip: u32,      // *const [u32; 4],
             tint: u32|
             -> anyhow::Result<()> {
                let (mem, state) = MainMemory::data_state_mut(&mut caller);

                let Some(renderer) = state.get_mut::<Self>() else {
                    anyhow::bail!("no renderer initialized");
                };

                let target = renderer.textures.get(target_id)?;
                let src = if src_id != 0 {
                    Some(renderer.textures.get(src_id)?)
                } else {
                    None
                };

                let transform = Affine2::from_cols_array(mem.mem_arr(transform)?);
                let clip = if clip != 0 {
                    let [x, y, w, h] = mem.mem_arr::<u32, 4>(clip)?.map(u32::from_le);

                    Some((UVec2::new(x, y), UVec2::new(w, h)))
                } else {
                    None
                };
                let tint = U8Vec4::from_array(tint.to_le_bytes());

                renderer
                    .context
                    .borrow_mut()
                    .draw_texture(target, src, transform, clip, tint)?;

                Ok(())
            },
        )?;

        linker.func_wrap(
            "crucible",
            "destroy_texture",
            |mut caller: wasmtime::Caller<'_, RtState>, handle: u32| -> anyhow::Result<()> {
                let Some(renderer) = Self::get_mut(&mut caller) else {
                    anyhow::bail!("no renderer initialized");
                };

                renderer.textures.remove(handle)?;

                Ok(())
            },
        )?;

        Ok(())
    }
}
