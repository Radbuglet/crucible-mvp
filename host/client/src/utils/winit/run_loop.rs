use std::{
    any::Any,
    cell::{Cell, RefCell},
    fmt,
    panic::{self, AssertUnwindSafe, Location},
    ptr::NonNull,
    rc::{Rc, Weak},
    sync::Arc,
    task,
};

use futures::{
    StreamExt,
    stream::FuturesUnordered,
    task::{LocalFutureObj, LocalSpawn, LocalSpawnExt, SpawnError},
};
use winit::{
    application::ApplicationHandler,
    event::{DeviceEvent, DeviceId, StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy},
    window::WindowId,
};

// === run_app_fallible === //

pub fn run_app_fallible(
    event_loop: EventLoop<()>,
    handler: &mut impl FallibleAppHandler,
) -> anyhow::Result<()> {
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

    struct Wrapper<'a, H> {
        handler: &'a mut H,
        _waker: Arc<WinitWaker>,
        erased_waker: task::Waker,
        futures: FuturesUnordered<LocalFutureObj<'static, ()>>,
        incoming: Rc<RefCell<Vec<LocalFutureObj<'static, ()>>>>,
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
        H: FallibleAppHandler,
    {
        fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
            self.exec_scoped(event_loop, |this| {
                this.handler.new_events(event_loop, cause)
            });
        }

        fn resumed(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.resumed(event_loop));
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
                this.handler.window_event(event_loop, window_id, event)
            });
        }

        fn device_event(
            &mut self,
            event_loop: &ActiveEventLoop,
            device_id: DeviceId,
            event: DeviceEvent,
        ) {
            self.exec_scoped(event_loop, |this| {
                this.handler.device_event(event_loop, device_id, event)
            });
        }

        fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| {
                this.handler.about_to_wait(event_loop)?;

                let _enter =
                    futures::executor::enter().expect("cannot run task executors reentrantly");

                provide_app_state_for_task(this.handler, || {
                    loop {
                        // Attempt to make progress.
                        _ = this
                            .futures
                            .poll_next_unpin(&mut task::Context::from_waker(&this.erased_waker));

                        // If that progress resulted in more tasks being spawned, try to update them.
                        let mut incoming = this.incoming.borrow_mut();
                        if !incoming.is_empty() {
                            this.futures.extend(incoming.drain(..));
                            continue;
                        }
                        drop(incoming);

                        break;
                    }
                });

                Ok(())
            });
        }

        fn suspended(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.suspended(event_loop));
        }

        fn exiting(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.exiting(event_loop));
        }

        fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
            self.exec_scoped(event_loop, |this| this.handler.memory_warning(event_loop));
        }
    }

    let incoming = Rc::new(RefCell::new(Vec::new()));
    let _incoming_guard = scopeguard::guard(
        CURRENT_TASK_SPAWNER.replace(Some(AppTaskSpawner {
            incoming: Rc::downgrade(&incoming),
        })),
        |old| CURRENT_TASK_SPAWNER.set(old),
    );

    let waker = Arc::new(WinitWaker {
        proxy: event_loop.create_proxy(),
    });
    let erased_waker = task::Waker::from(waker.clone());

    let mut app = Wrapper {
        handler,
        _waker: waker,
        erased_waker,
        futures: FuturesUnordered::new(),
        incoming,
        error: None,
    };

    event_loop.run_app(&mut app)?;

    if let Some(err) = app.error.take() {
        return Err(err);
    }

    Ok(())
}

pub trait FallibleAppHandler: Sized + 'static {
    fn new_events(
        &mut self,
        event_loop: &ActiveEventLoop,
        cause: StartCause,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, cause);

        Ok(())
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()>;

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) -> anyhow::Result<()>;

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: DeviceId,
        event: DeviceEvent,
    ) -> anyhow::Result<()> {
        let _ = (event_loop, device_id, event);

        Ok(())
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let _ = event_loop;

        Ok(())
    }

    fn acquire_in_task<R>(f: impl FnOnce(&mut Self) -> R) -> R {
        acquire_app_state_in_task(f)
    }
}

// === App State for Tasks === //

enum TaskState {
    Set(NonNull<dyn Any>),
    Unset,
    Borrowed(&'static Location<'static>),
}

thread_local! {
    static TASK_STATE: Cell<TaskState> = const { Cell::new(TaskState::Unset) };
}

pub fn provide_app_state_for_task<T: FallibleAppHandler, R>(
    state: &mut T,
    f: impl FnOnce() -> R,
) -> R {
    let _guard = scopeguard::guard(
        TASK_STATE.replace(TaskState::Set(NonNull::from(state as &mut dyn Any))),
        |old| {
            TASK_STATE.set(old);
        },
    );

    f()
}

#[track_caller]
pub fn acquire_app_state_in_task<T: FallibleAppHandler, R>(f: impl FnOnce(&mut T) -> R) -> R {
    let mut state = scopeguard::guard(
        TASK_STATE.replace(TaskState::Borrowed(Location::caller())),
        |old| {
            TASK_STATE.set(old);
        },
    );

    let state = match &mut *state {
        TaskState::Set(state) => state,
        TaskState::Unset => panic!("no app state bound"),
        TaskState::Borrowed(location) => panic!("app state already acquired at {location}"),
    };

    let state = unsafe { state.as_mut() }
        .downcast_mut::<T>()
        .expect("mismatched app types");

    f(state)
}

// === AppTaskSpawner === //

thread_local! {
    static CURRENT_TASK_SPAWNER: RefCell<Option<AppTaskSpawner>> = const { RefCell::new(None) };
}

#[derive(Clone)]
pub struct AppTaskSpawner {
    incoming: Weak<RefCell<Vec<LocalFutureObj<'static, ()>>>>,
}

impl AppTaskSpawner {
    pub fn get() -> Self {
        CURRENT_TASK_SPAWNER
            .with_borrow(|v| v.clone())
            .expect("no app running")
    }
}

impl fmt::Debug for AppTaskSpawner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppTaskSpawner").finish_non_exhaustive()
    }
}

impl LocalSpawn for AppTaskSpawner {
    fn spawn_local_obj(&self, future: LocalFutureObj<'static, ()>) -> Result<(), SpawnError> {
        self.incoming
            .upgrade()
            .ok_or_else(SpawnError::shutdown)?
            .borrow_mut()
            .push(future);

        Ok(())
    }
}

pub fn spawn_app_task(f: impl 'static + Future<Output = ()>) {
    AppTaskSpawner::get().spawn_local(f).unwrap();
}
