use std::time::{Duration, Instant};

use anyhow::Context;
use late_struct::late_field;

use crate::utils::wasm::{RtFieldExt, RtModule, RtState, RtStateNs};

#[derive(Debug)]
pub struct RtTime {
    start: Instant,
}

late_field!(RtTime[RtStateNs] => Option<RtTime>);

impl RtTime {
    pub fn init(
        store: &mut wasmtime::Store<RtState>,
        instance: wasmtime::Instance,
    ) -> anyhow::Result<()> {
        let _ = instance;

        *Self::get_mut(&mut *store) = Some(RtTime {
            start: Instant::now(),
        });

        Ok(())
    }

    pub fn encode_time(&self, instant: Instant) -> f64 {
        instant
            .checked_duration_since(self.start)
            .unwrap_or_else(|| self.start.duration_since(instant))
            .as_secs_f64()
    }

    pub fn decode_time(&self, time: f64) -> anyhow::Result<Instant> {
        Ok(if time < 0. {
            self.start
                .checked_sub(Duration::try_from_secs_f64(-time)?)
                .context("time underflow")?
        } else {
            self.start
                .checked_add(Duration::try_from_secs_f64(time)?)
                .context("time overflow")?
        })
    }
}

impl RtModule for RtTime {
    fn define(linker: &mut wasmtime::Linker<RtState>) -> anyhow::Result<()> {
        linker.func_wrap(
            "crucible",
            "current_time",
            |mut caller: wasmtime::Caller<RtState>| -> anyhow::Result<f64> {
                let Some(state) = Self::get_mut(&mut caller) else {
                    anyhow::bail!("time module never initialized");
                };

                Ok(state.encode_time(Instant::now()))
            },
        )?;

        Ok(())
    }
}
