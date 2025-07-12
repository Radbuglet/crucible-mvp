use std::{fmt, sync::Arc};

use anyhow::Context as _;
use late_struct::late_field;

use crate::utils::wasm::{MainMemory, RtFieldExt, RtModule, RtState, RtStateNs};

pub type LogCallback = Arc<dyn Send + Sync + Fn(&mut RtState, &str)>;

#[derive(Default)]
pub struct RtLogger {
    callback: Option<LogCallback>,
}

impl fmt::Debug for RtLogger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RtLogger").finish_non_exhaustive()
    }
}

late_field!(RtLogger[RtStateNs]);

impl RtLogger {
    pub fn init(store: &mut wasmtime::Store<RtState>, cb: LogCallback) -> anyhow::Result<()> {
        Self::get_mut(store).callback = Some(cb);

        Ok(())
    }
}

impl RtModule for RtLogger {
    fn define(linker: &mut wasmtime::Linker<RtState>) -> anyhow::Result<()> {
        linker.func_wrap(
            "crucible",
            "log",
            |mut caller: wasmtime::Caller<'_, RtState>,
             level: u32,
             base: u32,
             len: u32|
             -> anyhow::Result<()> {
                let (memory, state) = MainMemory::data_state_mut(&mut caller);

                let Some(cb) = state.get::<Self>().callback.clone() else {
                    anyhow::bail!("logger API not enabled");
                };

                let msg = memory
                    .get(base as usize..)
                    .and_then(|v| v.get(..len as usize))
                    .context("failed to get message")?;

                let msg = std::str::from_utf8(msg).context("malformed log message")?;

                cb(state, msg);

                Ok(())
            },
        )?;

        Ok(())
    }
}
