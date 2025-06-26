use derive_where::derive_where;
use late_struct::late_field;

use crate::runtime::base::RtStateNs;

use super::base::{RtFieldExt, RtModule, RtState};

#[derive_where(Debug)]
pub struct RtMainLoop {
    exit_confirmed: bool,

    #[derive_where(skip)]
    dispatch_redraw: wasmtime::TypedFunc<(), ()>,

    #[derive_where(skip)]
    dispatch_exit_request: wasmtime::TypedFunc<(), ()>,
}

late_field!(RtMainLoop[RtStateNs] => Option<RtMainLoop>);

impl RtModule for RtMainLoop {
    fn define(linker: &mut wasmtime::Linker<RtState>) -> anyhow::Result<()> {
        linker.func_wrap(
            "crucible",
            "confirm_app_exit",
            |mut caller: wasmtime::Caller<RtState>| -> anyhow::Result<()> {
                let Some(loop_state) = Self::get_mut(&mut caller) else {
                    anyhow::bail!("run loop service not initialized");
                };

                loop_state.exit_confirmed = true;
                Ok(())
            },
        )?;

        Ok(())
    }
}

impl RtMainLoop {
    pub fn init(
        store: &mut wasmtime::Store<RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        *Self::get_mut(&mut *store) = Some(RtMainLoop {
            exit_confirmed: false,
            dispatch_redraw: instance.get_typed_func(&mut *store, "crucible_dispatch_redraw")?,
            dispatch_exit_request: instance
                .get_typed_func(&mut *store, "crucible_dispatch_request_exit")?,
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

    pub fn dispatch_exit_request(store: &mut wasmtime::Store<RtState>) -> anyhow::Result<()> {
        Self::get(&mut *store)
            .as_ref()
            .unwrap()
            .dispatch_exit_request
            .clone()
            .call(&mut *store, ())
    }

    pub fn is_exit_confirmed(store: &wasmtime::Store<RtState>) -> bool {
        Self::get(store).as_ref().unwrap().exit_confirmed
    }
}
