use std::{
    cmp::Ordering,
    collections::BTreeMap,
    num::FpCategory,
    time::{Duration, Instant},
};

use anyhow::Context;
use arid::{Handle, Strong, W, Wr};
use arid_entity::{Component as _, EntityHandle, component};
use crucible_abi::{self as abi, RunMode};
use crucible_host_shared::guest::arena::GuestArena;
use wasmlink_wasmtime::{WslContext, WslLinker, WslLinkerExt};

#[derive(Debug)]
pub struct EnvBindings {
    epoch: Instant,
    timeout_handles: GuestArena<f64>,
    timeout_queue: BTreeMap<IdentifiedTimeout, wasmlink::HostClosure<()>>,
}

#[derive(Debug)]
struct IdentifiedTimeout {
    expires_at: f64,
    handle: u32,
}

impl Eq for IdentifiedTimeout {}

impl PartialEq for IdentifiedTimeout {
    fn eq(&self, other: &Self) -> bool {
        self.expires_at == other.expires_at && self.handle == other.handle
    }
}

impl Ord for IdentifiedTimeout {
    fn cmp(&self, other: &Self) -> Ordering {
        let cmp = f64::total_cmp(&self.expires_at, &other.expires_at);

        if !cmp.is_eq() {
            return cmp;
        }

        self.handle.cmp(&other.handle)
    }
}

impl PartialOrd for IdentifiedTimeout {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

component!(pub EnvBindings);

impl EnvBindingsHandle {
    pub fn new(owner: EntityHandle, w: W) -> Strong<Self> {
        EnvBindings {
            epoch: Instant::now(),
            timeout_handles: GuestArena::default(),
            timeout_queue: BTreeMap::default(),
        }
        .attach(owner, w)
    }

    pub fn install(self, linker: &mut WslLinker) -> anyhow::Result<()> {
        linker.define_wsl(abi::GET_RUN_MODE, |cx, (), out| {
            out.finish(cx, &RunMode::Client)
        })?;

        linker.define_wsl(abi::LOG_MESSAGE, |cx, msg, out| {
            tracing::info!(
                target = "guest",
                file = msg.file.read(cx)?,
                line = msg.line,
                column = msg.column,
                "{}",
                msg.msg.read(cx)?
            );

            out.finish(cx, &())
        })?;

        linker.define_wsl(abi::GET_CURRENT_TIME, move |cx, (), out| {
            out.finish(cx, &self.current_time(cx.wr()))
        })?;

        linker.define_wsl(abi::SPAWN_TIMEOUT, move |cx, req, out| {
            let w = cx.w();

            let expires_at = match req.expires_at.classify() {
                FpCategory::Zero | FpCategory::Normal => req.expires_at,
                FpCategory::Subnormal => 0.0,
                FpCategory::Nan | FpCategory::Infinite => {
                    anyhow::bail!("invalid timeout {:?}", req.expires_at)
                }
            };

            let handle = self.m(w).timeout_handles.add(expires_at)?;

            self.m(w)
                .timeout_queue
                .insert(IdentifiedTimeout { expires_at, handle }, req.handler);

            out.finish(cx, &abi::TimeoutHandle { raw: handle })
        })?;

        linker.define_wsl(abi::CLEAR_TIMEOUT, move |cx, req, out| {
            let w = cx.w();

            let time = self
                .m(w)
                .timeout_handles
                .remove(req.raw)
                .context("invalid timeout handle")?;

            self.m(w).timeout_queue.remove(&IdentifiedTimeout {
                expires_at: time,
                handle: req.raw,
            });

            out.finish(cx, &())
        })?;

        Ok(())
    }

    pub fn current_time(self, w: Wr) -> f64 {
        self.r(w).epoch.elapsed().as_secs_f64()
    }

    pub fn earliest_timeout(self, w: Wr) -> Option<Instant> {
        let me = self.r(w);

        let (IdentifiedTimeout { expires_at, .. }, _) = me.timeout_queue.first_key_value()?;

        me.epoch.checked_add(Duration::from_secs_f64(*expires_at))
    }

    pub fn poll_timeouts(self, cx: &mut WslContext<'_>) -> anyhow::Result<()> {
        let now = self.current_time(cx.w());

        while let Some(first) = self.m(cx.w()).timeout_queue.first_entry() {
            let IdentifiedTimeout { expires_at, handle } = *first.key();

            if expires_at > now {
                break;
            }

            let callback = first.remove();

            self.m(cx.w()).timeout_handles.remove(handle)?;

            callback.call(cx, &())?;
        }

        Ok(())
    }
}
