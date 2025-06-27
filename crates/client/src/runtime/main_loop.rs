use late_struct::late_field;

use std::fmt;

use crate::runtime::base::RtStateNs;

use super::base::{RtFieldExt, RtModule, RtState};

#[derive(Debug)]
pub struct RtMainLoop {
    exit_confirmed: bool,
    redraw_requested: bool,
    hooks: Hooks,
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

        linker.func_wrap(
            "crucible",
            "request_redraw",
            |mut caller: wasmtime::Caller<RtState>| -> anyhow::Result<()> {
                let Some(loop_state) = Self::get_mut(&mut caller) else {
                    anyhow::bail!("run loop service not initialized");
                };

                loop_state.redraw_requested = true;

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
            redraw_requested: false,
            hooks: Hooks::init(store, instance)?,
        });

        Ok(())
    }

    pub fn is_exit_confirmed(store: &wasmtime::Store<RtState>) -> bool {
        Self::get(store).as_ref().unwrap().exit_confirmed
    }

    pub fn is_redraw_requested(store: &wasmtime::Store<RtState>) -> bool {
        Self::get(store).as_ref().unwrap().redraw_requested
    }
}

macro_rules! define_hooks {
    (
        $(
            $name:ident (
                $($arg_name:ident: $arg:ty),*$(,)?
            ): $func:expr
        ),*
        $(,)?
    ) => {
        struct Hooks {
            $($name: wasmtime::TypedFunc<($($arg,)*), ()>,)*
        }

        impl fmt::Debug for Hooks {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_struct("Hooks").finish_non_exhaustive()
            }
        }

        impl Hooks {
            fn init(
                store: &mut wasmtime::Store<RtState>,
                instance: wasmtime::Instance,
            ) -> anyhow::Result<Self> {
                Ok(Self {$(
                    $name: instance.get_typed_func(&mut *store, $func)?,
                )*})
            }
        }

        impl RtMainLoop {$(
            pub fn $name(store: &mut wasmtime::Store<RtState>, $($arg_name: $arg,)*) -> anyhow::Result<()> {
                Self::get(&mut *store)
                    .as_ref()
                    .unwrap()
                    .hooks
                    .$name
                    .clone()
                    .call(&mut *store, ($($arg_name,)*))
            }
        )*}
    };
}

define_hooks! {
    redraw(): "crucible_dispatch_redraw",
    request_exit(): "crucible_dispatch_request_exit",
    mouse_moved(x: f64, y: f64): "crucible_dispatch_mouse_moved",
}
