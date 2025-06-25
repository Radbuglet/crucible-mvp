use derive_where::derive_where;
use late_struct::late_field;

use crate::runtime::base::RtStateNs;

use super::base::{RtFieldExt, RtModule, RtState};

#[derive_where(Debug)]
pub struct RtMainLoop {
    #[derive_where(skip)]
    dispatch_redraw: wasmtime::TypedFunc<(), ()>,
}

late_field!(RtMainLoop[RtStateNs] => Option<RtMainLoop>);

impl RtModule for RtMainLoop {
    fn define(_linker: &mut wasmtime::Linker<RtState>) -> anyhow::Result<()> {
        Ok(())
    }
}

impl RtMainLoop {
    pub fn init(
        store: &mut wasmtime::Store<RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        *Self::get_mut(&mut *store) = Some(RtMainLoop {
            dispatch_redraw: instance.get_typed_func(&mut *store, "crucible_dispatch_redraw")?,
        });

        Ok(())
    }

    pub fn dispatch_redraw(store: &mut wasmtime::Store<RtState>) -> anyhow::Result<()> {
        Self::get(&mut *store)
            .as_ref()
            .unwrap()
            .dispatch_redraw
            .clone()
            .call(&mut *store, ())
    }
}
