use std::{cell::Cell, fmt, future, panic::Location, pin::Pin, ptr::NonNull, rc::Rc, task};

use derive_where::derive_where;

// === Structures === //

pub struct BackgroundTasksExecutor<S: 'static, T: 'static, O: 'static> {
    inner_stateful: BackgroundTasks<S, T>,
    exec_future: Pin<Box<dyn Future<Output = anyhow::Result<O>>>>,
}

#[derive_where(Clone)]
pub struct BackgroundTasks<S: 'static, T: 'static> {
    shared: Rc<Cell<StatefulShared<S, T>>>,
    inner_erased: BackgroundTasksErased,
}

#[derive(Clone)]
pub struct BackgroundTasksErased {
    shared: Rc<ErasedShared>,
}

enum StatefulShared<S: 'static, T: 'static> {
    Set(NonNull<S>, NonNull<T>),
    Unset,
    Borrowed(&'static Location<'static>),
}

struct ErasedShared {
    error: Cell<Option<anyhow::Error>>,
    executor: smol::LocalExecutor<'static>,
}

// === BackgroundTasksExecutor === //

impl<S: 'static, T: 'static, O: 'static> BackgroundTasksExecutor<S, T, O> {
    pub fn handle(&self) -> &BackgroundTasks<S, T> {
        &self.inner_stateful
    }

    pub fn poll(
        &mut self,
        event_loop: &S,
        app: &mut T,
        cx: &mut task::Context,
    ) -> task::Poll<anyhow::Result<O>> {
        let res = self
            .inner_stateful
            .provide_state(event_loop, app, || self.exec_future.as_mut().poll(cx));

        if let task::Poll::Ready(res) = res {
            return task::Poll::Ready(res);
        }

        if let Some(err) = self.inner_stateful.inner_erased.shared.error.take() {
            return task::Poll::Ready(Err(err));
        }

        task::Poll::Pending
    }

    pub async fn future(&mut self, event_loop: &S, app: &mut T) -> anyhow::Result<O> {
        future::poll_fn(|cx| self.poll(event_loop, app, cx)).await
    }
}

// === BackgroundTasksState === //

impl<S: 'static, T: 'static> fmt::Debug for BackgroundTasks<S, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackgroundTasksState")
            .finish_non_exhaustive()
    }
}

// Conversions
impl<S: 'static, T: 'static> BackgroundTasks<S, T> {
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        let shared = Rc::new(ErasedShared {
            error: Cell::new(None),
            executor: smol::LocalExecutor::new(),
        });

        Self {
            inner_erased: BackgroundTasksErased { shared },
            shared: Rc::new(Cell::new(StatefulShared::Unset)),
        }
    }

    pub fn executor<O: 'static>(
        self,
        main: impl 'static + Future<Output = anyhow::Result<O>>,
    ) -> BackgroundTasksExecutor<S, T, O> {
        let exec_future = Box::pin({
            let inner = self.inner_erased.clone();

            async move { inner.shared.executor.run(main).await }
        });

        BackgroundTasksExecutor {
            inner_stateful: self,
            exec_future,
        }
    }

    pub fn erased(&self) -> &BackgroundTasksErased {
        &self.inner_erased
    }

    pub fn into_erased(self) -> BackgroundTasksErased {
        self.inner_erased
    }
}

// Stateful ops
impl<S: 'static, T: 'static> BackgroundTasks<S, T> {
    pub fn provide_state<R>(&self, event_loop: &S, state: &mut T, f: impl FnOnce() -> R) -> R {
        let _guard_state = scopeguard::guard(
            self.shared.replace(StatefulShared::Set(
                NonNull::from(event_loop),
                NonNull::from(state),
            )),
            |old| {
                self.shared.set(old);
            },
        );

        f()
    }

    #[track_caller]
    pub fn acquire_state<R>(&self, f: impl FnOnce(&S, &mut T) -> R) -> R {
        let mut state = scopeguard::guard(
            self.shared
                .replace(StatefulShared::Borrowed(Location::caller())),
            |old| {
                self.shared.set(old);
            },
        );

        let (event_loop, state) = match &mut *state {
            StatefulShared::Set(a, b) => (a, b),
            StatefulShared::Unset => panic!("no app state bound"),
            StatefulShared::Borrowed(location) => {
                panic!("app state already acquired at {location}")
            }
        };

        f(unsafe { event_loop.as_mut() }, unsafe { state.as_mut() })
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
}

// Forwards
impl<S: 'static, T: 'static> BackgroundTasks<S, T> {
    pub fn spawn<O: 'static>(&self, f: impl 'static + Future<Output = O>) -> smol::Task<O> {
        self.erased().spawn(f)
    }

    pub fn spawn_fallible<O: 'static>(
        &self,
        f: impl 'static + Future<Output = anyhow::Result<O>>,
    ) -> smol::Task<Option<O>> {
        self.erased().spawn_fallible(f)
    }

    pub fn report_error(&self, error: anyhow::Error) {
        self.erased().report_error(error);
    }
}

// === BackgroundTasksErased === //

impl fmt::Debug for BackgroundTasksErased {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BackgroundTasksErased")
            .finish_non_exhaustive()
    }
}

impl BackgroundTasksErased {
    pub fn spawn<O: 'static>(&self, f: impl 'static + Future<Output = O>) -> smol::Task<O> {
        self.shared.executor.spawn(f)
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

    pub fn report_error(&self, error: anyhow::Error) {
        self.shared.error.set(Some(error));
    }
}
