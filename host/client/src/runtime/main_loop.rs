use crucible_shared::{
    runtime::RtTime,
    utils::wasm::{RtFieldExt, RtModule, RtOptFieldExt, RtState, RtStateNs, StoreDataMut},
};
use late_struct::late_field;

use std::{fmt, mem, time::Instant};

#[derive(Debug)]
pub struct RtMainLoop {
    exit_confirmed: bool,
    redraw_requested: bool,
    wakeup: Option<Instant>,
    hooks: Hooks,
}

late_field!(RtMainLoop[RtStateNs] => Option<RtMainLoop>);

impl RtMainLoop {
    pub fn init(
        store: &mut wasmtime::Store<RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        *Self::get_mut(&mut *store) = Some(Self {
            exit_confirmed: false,
            redraw_requested: false,
            wakeup: None,
            hooks: Hooks::init(store, instance)?,
        });

        Ok(())
    }

    pub fn take_exit_confirmed(store: &mut impl StoreDataMut<Data = RtState>) -> bool {
        mem::take(&mut Self::get_unwrap_mut(store).exit_confirmed)
    }

    pub fn take_redraw(store: &mut impl StoreDataMut<Data = RtState>) -> bool {
        mem::take(&mut Self::get_unwrap_mut(store).redraw_requested)
    }

    pub fn take_wakeup(store: &mut impl StoreDataMut<Data = RtState>) -> Option<Instant> {
        mem::take(&mut Self::get_unwrap_mut(store).wakeup)
    }
}

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

        linker.func_wrap(
            "crucible",
            "request_loop_wakeup",
            |mut caller: wasmtime::Caller<RtState>, time: f64| -> anyhow::Result<()> {
                if Self::get_mut(&mut caller).is_none() {
                    anyhow::bail!("run loop service not initialized");
                }

                Self::get_unwrap_mut(&mut caller).wakeup =
                    Some(RtTime::get_unwrap(&caller).decode_time(time)?);

                Ok(())
            },
        )?;

        Ok(())
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

        #[allow(clippy::too_many_arguments)]
        impl RtMainLoop {$(
            pub fn $name(store: &mut wasmtime::Store<RtState>, $($arg_name: $arg,)*) -> anyhow::Result<()> {
                Self::get_unwrap(&mut *store)
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
    timer_expired(): "crucible_dispatch_timer_expired",
    key_event(
        physical_key: u32,
        logical_key_as_str: u32,
        logical_key_as_str_len: u32,
        logical_key_as_named: u32,
        text: u32,
        text_len: u32,
        location: u32,
        pressed: u32,
        repeat: u32,
    ): "crucible_dispatch_key_event"
}
