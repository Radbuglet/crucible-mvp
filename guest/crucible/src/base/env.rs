use std::{
    cell::{Cell, OnceCell},
    num::NonZeroU32,
};

use futures::channel::oneshot;
use scopeguard::ScopeGuard;
use wasmlink::{OwnedGuestClosure, bind_port};

use crate::base::task::wake_executor;

// === RunMode === //

#[derive(Debug, Copy, Clone, Hash, Eq, PartialEq)]
pub enum RunMode {
    Server,
    Client,
}

impl RunMode {
    pub fn get() -> Self {
        thread_local! {
            static CACHE: OnceCell<RunMode> = const { OnceCell::new() };
        }

        CACHE.with(|v| {
            *v.get_or_init(|| {
                bind_port! {
                    fn [crucible_abi::GET_RUN_MODE] "crucible".get_run_mode(()) -> crucible_abi::RunMode;
                }

                match get_run_mode(&()) {
                    crucible_abi::RunMode::Server => Self::Server,
                    crucible_abi::RunMode::Client => Self::Client,
                }
            })
        })
    }

    pub fn is_server(self) -> bool {
        self == Self::Server
    }

    pub fn is_client(self) -> bool {
        self == Self::Client
    }

    #[track_caller]
    pub fn assert_client(self) {
        assert!(
            self.is_client(),
            "unsupported platform—expected to be running on client"
        );
    }

    #[track_caller]
    pub fn assert_server(self) {
        assert!(
            self.is_server(),
            "unsupported platform—expected to be running on server"
        );
    }
}

// === Time === //

pub fn current_time() -> f64 {
    bind_port! {
        fn [crucible_abi::GET_CURRENT_TIME] "crucible".get_current_time(()) -> f64;
    }

    get_current_time(&())
}

pub async fn wait_until(expires_at: f64) {
    bind_port! {
        fn [crucible_abi::SPAWN_TIMEOUT] "crucible".spawn_timeout(crucible_abi::SpawnTimeoutArgs)
            -> crucible_abi::TimeoutHandle;

        fn [crucible_abi::CLEAR_TIMEOUT] "crucible".clear_timeout(crucible_abi::TimeoutHandle);
    }

    let (tx, rx) = oneshot::channel();
    let tx = Cell::new(Some(tx));

    let callback = OwnedGuestClosure::<()>::new(move |()| {
        tx.take().unwrap().send(()).unwrap();
        wake_executor();
    });

    let handle = spawn_timeout(&crucible_abi::SpawnTimeoutArgs {
        handler: callback.handle(),
        expires_at,
    });

    let cancel_guard = scopeguard::guard((), |()| {
        clear_timeout(&handle);
    });

    rx.await.unwrap();

    ScopeGuard::into_inner(cancel_guard);
}

#[derive(Debug)]
pub struct IntervalTimer {
    interval: f64,
    last_complete: f64,
}

impl IntervalTimer {
    pub fn new(interval: f64) -> Self {
        Self {
            interval,
            last_complete: current_time(),
        }
    }

    #[must_use]
    pub fn unprocessed(&self) -> f64 {
        (current_time() - self.last_complete) / self.interval
    }

    #[must_use]
    pub async fn next(&mut self) -> (NonZeroU32, f64) {
        loop {
            let events = self.unprocessed();

            let Some(event_discrete) = NonZeroU32::new(events as u32) else {
                wait_until(self.last_complete + self.interval).await;
                continue;
            };

            self.last_complete += events.trunc() * self.interval;

            break (event_discrete, events.fract());
        }
    }
}
