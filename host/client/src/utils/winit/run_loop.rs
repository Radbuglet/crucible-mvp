use std::{
    cell::Cell,
    fmt, future,
    panic::{self, AssertUnwindSafe, Location},
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    sync::Arc,
    task,
};

use derive_where::derive_where;
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

pub fn run_winit(event_loop: EventLoop<()>, handler: &mut impl WinitHandler) -> anyhow::Result<()> {
    struct WinitWaker {
        // TODO: Avoid redundant wake-ups.
        proxy: EventLoopProxy<()>,
    }

    impl task::Wake for WinitWaker {
        fn wake(self: Arc<Self>) {
            self.wake_by_ref();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            _ = self.proxy.send_event(());
        }
    }

    struct Wrapper<'a, H: 'static> {
        handler: &'a mut H,
        _waker: Arc<WinitWaker>,
        erased_waker: task::Waker,
        background: BackgroundTasks<H>,
        executor_future: Pin<Box<dyn Future<Output = ()>>>,
        error: Option<anyhow::Error>,
    }

    impl<H> Wrapper<'_, H> {
        fn exec_scoped(
            &mut self,
            event_loop: &ActiveEventLoop,
            f: impl FnOnce(&mut Self) -> anyhow::Result<()>,
        ) {
            if self.error.is_some() {
                return;
            }

            match panic::catch_unwind(AssertUnwindSafe(|| f(self))) {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    self.error = Some(e);
                    event_loop.exit();
                }
                Err(_) => {
                    self.error = Some(anyhow::anyhow!("run loop panicked"));
                    event_loop.exit();
                }
            }
        }
    }

    impl<H> ApplicationHandler for Wrapper<'_, H>
    where
        H: WinitHandler,
    {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            self.exec_scoped(event_loop, |this| {
                this.handler.new_events(event_loop, &this.background, cause)
            });
        }

        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.resumed(event_loop, &this.background)
            });
        }

        fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: ()) {
            // (just used for wakeups)
        }

        fn window_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            window_id: WindowId,
            event: WindowEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .window_event(event_loop, &this.background, window_id, event)
            });
        }

        fn device_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            device_id: DeviceId,
            event: DeviceEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler
                    .device_event(event_loop, &this.background, device_id, event)
            });
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.about_to_wait(event_loop, &this.background)?;

                this.background.provide_state(event_loop, this.handler, || {
                    let res = this
                        .executor_future
                        .as_mut()
                        .poll(&mut task::Context::from_waker(&this.erased_waker));

                    debug_assert!(res.is_pending());
                });

                if let Some(err) = this.background.inner.error.take() {
                    return Err(err);
                }

                Ok(())
            });
        }

        fn suspended(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.suspended(event_loop, &this.background)
            });
        }

        fn exiting(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.exiting(event_loop, &this.background)
            });
        }

        fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.memory_warning(event_loop, &this.background)
            });
        }
    }

    let background = BackgroundTasks {
        inner: Rc::new(BackgroundTasksInner {
            state: Cell::new(TaskAppState::Unset),
            error: Cell::new(None),
            executor: smol::LocalExecutor::new(),
        }),
    };

    let executor_future = Box::pin({
        let background = background.clone();
        async move { background.inner.executor.run(future::pending::<()>()).await }
    });

    let waker = Arc::new(WinitWaker {
        proxy: event_loop.create_proxy(),
    });
    let erased_waker = task::Waker::from(waker.clone());

    let mut app = Wrapper {
        handler,
        _waker: waker,
        erased_waker,
        background,
        executor_future,
        error: None,
    };

    event_loop.run_app(&mut app)?;

    if let Some(err) = app.error.take() {
        return Err(err);
    }

    Ok(())
}

pub trait WinitHandler: Sized + 'static {
    fn new_events(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
        cause: StartCause,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background, cause);

        Ok(())
    }

    fn resumed(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()>;

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()>;

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
        device_id: DeviceId,
        event: DeviceEvent,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background, device_id, event);

        Ok(())
    }

    fn about_to_wait(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }

    fn suspended(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }

    fn exiting(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }

    fn memory_warning(
        &mut self,
        event_loop: &ActiveEventLoop,
        background: &BackgroundTasks<Self>,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, background);

        Ok(())
    }
}

#[derive_where(Clone)]
pub struct BackgroundTasks<T: 'static> {
    inner: Rc<BackgroundTasksInner<T>>,
}

struct BackgroundTasksInner<T> {
    state: Cell<TaskAppState<T>>,
    error: Cell<Option<anyhow::Error>>,
    executor: smol::LocalExecutor<'static>,
}

enum TaskAppState<T> {
    Set(NonNull<ActiveEventLoop>, NonNull<T>),
    Unset,
    Borrowed(&'static Location<'static>),
}

impl<T: 'static> fmt::Debug for BackgroundTasks<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackgroundTasks").finish_non_exhaustive()
    }
}

impl<T: 'static> BackgroundTasks<T> {
    fn provide_state<R>(
        &self,
        event_loop: &ActiveEventLoop,
        state: &mut T,
        f: impl FnOnce() -> R,
    ) -> R {
        let _guard_state = scopeguard::guard(
            self.inner.state.replace(TaskAppState::Set(
                NonNull::from(event_loop),
                NonNull::from(state),
            )),
            |old| {
                self.inner.state.set(old);
            },
        );

        f()
    }

    #[track_caller]
    pub fn acquire_state<R>(&self, f: impl FnOnce(&ActiveEventLoop, &mut T) -> R) -> R {
        let mut state = scopeguard::guard(
            self.inner
                .state
                .replace(TaskAppState::Borrowed(Location::caller())),
            |old| {
                self.inner.state.set(old);
            },
        );

        let (event_loop, state) = match &mut *state {
            TaskAppState::Set(a, b) => (a, b),
            TaskAppState::Unset => panic!("no app state bound"),
            TaskAppState::Borrowed(location) => panic!("app state already acquired at {location}"),
        };

        f(unsafe { event_loop.as_mut() }, unsafe { state.as_mut() })
    }

    pub fn spawn<O: 'static>(&self, f: impl 'static + Future<Output = O>) -> smol::Task<O> {
        self.inner.executor.spawn(f)
    }

    pub fn spawn_fallible<O: 'static>(
        &self,
        f: impl 'static + Future<Output = anyhow::Result<O>>,
    ) -> smol::Task<Option<O>> {
        let me = self.clone();

        self.spawn(async move {
            match f.await {
                Ok(val) => Some(val),
                Err(err) => {
                    me.report_error(err);
                    None
                }
            }
        })
    }

    pub fn spawn_responder<V, O>(
        &self,
        fut: impl 'static + Future<Output = V>,
        resp: impl 'static + FnOnce(&ActiveEventLoop, &mut T, V) -> anyhow::Result<O>,
    ) -> smol::Task<Option<O>>
    where
        V: 'static,
        O: 'static,
    {
        let me = self.clone();

        self.spawn_fallible(async move {
            let res = fut.await;
            me.acquire_state(|event_loop, state| resp(event_loop, state, res))
        })
    }

    fn report_error(&self, error: anyhow::Error) {
        self.inner.error.set(Some(error));
    }
}
