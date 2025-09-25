use std::{cell::Cell, fmt, future, panic::Location, pin::Pin, ptr::NonNull, rc::Rc, task};

use derive_where::derive_where;

#[derive_where(Clone)]
pub struct BackgroundTasks<S: 'static, T: 'static> {
    inner: Rc<BackgroundTasksInner<S, T>>,
}

struct BackgroundTasksInner<S: 'static, T: 'static> {
    state: Cell<TaskAppState<S, T>>,
    error: Cell<Option<anyhow::Error>>,
    executor: smol::LocalExecutor<'static>,
}

enum TaskAppState<S: 'static, T: 'static> {
    Set(NonNull<S>, NonNull<T>),
    Unset,
    Borrowed(&'static Location<'static>),
}

impl<S: 'static, T: 'static> fmt::Debug for BackgroundTasks<S, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackgroundTasks").finish_non_exhaustive()
    }
}

impl<S: 'static, T: 'static> BackgroundTasks<S, T> {
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        let inner = Rc::new(BackgroundTasksInner {
            state: Cell::new(TaskAppState::Unset),
            error: Cell::new(None),
            executor: smol::LocalExecutor::new(),
        });

        Self { inner }
    }

    pub fn executor<O: 'static>(
        self,
        main: impl 'static + Future<Output = anyhow::Result<O>>,
    ) -> BackgroundTaskExecutor<S, T, O> {
        let exec_future = Box::pin({
            let inner = self.inner.clone();

            async move { inner.executor.run(main).await }
        });

        BackgroundTaskExecutor {
            handle: self,
            exec_future,
        }
    }

    pub fn provide_state<R>(&self, event_loop: &S, state: &mut T, f: impl FnOnce() -> R) -> R {
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
    pub fn acquire_state<R>(&self, f: impl FnOnce(&S, &mut T) -> R) -> R {
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
        resp: impl 'static + FnOnce(&S, &mut T, V) -> anyhow::Result<O>,
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

pub struct BackgroundTaskExecutor<S: 'static, T: 'static, O: 'static> {
    handle: BackgroundTasks<S, T>,
    exec_future: Pin<Box<dyn Future<Output = anyhow::Result<O>>>>,
}

impl<S: 'static, T: 'static, O: 'static> BackgroundTaskExecutor<S, T, O> {
    pub fn handle(&self) -> &BackgroundTasks<S, T> {
        &self.handle
    }

    pub fn poll(
        &mut self,
        event_loop: &S,
        app: &mut T,
        cx: &mut task::Context,
    ) -> task::Poll<anyhow::Result<O>> {
        let res = self
            .handle
            .provide_state(event_loop, app, || self.exec_future.as_mut().poll(cx));

        if let task::Poll::Ready(res) = res {
            return task::Poll::Ready(res);
        }

        if let Some(err) = self.handle.inner.error.take() {
            return task::Poll::Ready(Err(err));
        }

        task::Poll::Pending
    }

    pub async fn future(&mut self, event_loop: &S, app: &mut T) -> anyhow::Result<O> {
        future::poll_fn(|cx| self.poll(event_loop, app, cx)).await
    }
}
