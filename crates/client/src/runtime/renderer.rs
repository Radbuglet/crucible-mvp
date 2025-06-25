use std::{cell::RefCell, rc::Rc};

use anyhow::Context;
use crucible_renderer::GfxContext;
use late_struct::late_field;

use crate::{
    runtime::base::RtStateNs,
    utils::{arena::Arena, memory::MemoryExt},
};

use super::base::{MainMemory, RtFieldExt, RtModule, RtState};

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
                let (data, state) = MainMemory::data_state_mut(&mut caller);

                let Some(renderer) = state.get_mut::<Self>() else {
                    anyhow::bail!("no renderer initialized");
                };

                let texture = renderer
                    .swapchain_texture
                    .clone()
                    .context("no swapchain actively bound")?;

                data.mem_elem_mut(out_size, 2)?
                    .copy_from_slice(&[texture.width().to_le(), texture.height().to_le()]);

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
