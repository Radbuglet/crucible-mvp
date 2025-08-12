use arid::{Object, Strong, W};
use arid_entity::component;
use wasmlink_wasmtime::WslLinker;

#[derive(Debug)]
pub struct GfxBindings {}

component!(pub GfxBindings);

impl GfxBindingsHandle {
    pub fn new(w: W) -> Strong<Self> {
        GfxBindings {}.spawn(w)
    }

    pub fn install(self, linker: &mut WslLinker) -> anyhow::Result<()> {
        Ok(())
    }
}
