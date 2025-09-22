use std::{
    cell::Cell,
    fmt, future,
    pin::{Pin, pin},
    rc::Rc,
    task, thread,
};

use thiserror::Error;

// === Shared === //

#[derive(Debug, Clone, Error)]
#[error("{}", if self.was_panic { "worker thread panicked" } else { "promise dropped unexpectedly" })]
pub struct PromiseCrashed {
    pub was_panic: bool,
}

struct State<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    cancelled: Cell<bool>,
    on_cancel_waker: Cell<Option<task::Waker>>,
    result: Cell<Option<Result<T, E>>>,
    on_result_waker: Cell<Option<task::Waker>>,
}

pub fn promise<T, E>() -> (Promise<T, E>, PromiseFuture<T, E>)
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    let state = Rc::new(State {
        cancelled: Cell::new(false),
        on_cancel_waker: Cell::new(None),
        result: Cell::new(None),
        on_result_waker: Cell::new(None),
    });

    (
        Promise {
            resolver: PromiseResolver {
                state: Some(state.clone()),
            },
        },
        PromiseFuture { state: Some(state) },
    )
}

// === Promise === //

pub struct Promise<T, E = PromiseCrashed>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    resolver: PromiseResolver<T, E>,
}

impl<T, E> fmt::Debug for Promise<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Promise").finish_non_exhaustive()
    }
}

impl<T, E> Promise<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    pub async fn resolve_cancellable(self, f: impl Future<Output = Result<T, E>>) -> bool {
        let mut f = pin!(f);
        let (resolver, mut cancelled) = self.split();
        let mut resolver = Some(resolver);

        future::poll_fn(|cx| {
            // See whether we've been cancelled yet...
            if Pin::new(&mut cancelled).poll(cx).is_ready() {
                return task::Poll::Ready(false);
            }

            // Otherwise, make progress on the actual future.
            if let task::Poll::Ready(res) = f.as_mut().poll(cx) {
                resolver.take().unwrap().finish(res);
                return task::Poll::Ready(true);
            }

            task::Poll::Pending
        })
        .await
    }

    pub fn split(self) -> (PromiseResolver<T, E>, PromiseCancelledFuture<T, E>) {
        let cancelled = PromiseCancelledFuture {
            state: self.resolver.state.clone().unwrap(),
        };

        (self.resolver, cancelled)
    }
}

impl<T, E> Promise<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    pub fn is_cancelled(&self) -> bool {
        self.resolver.is_cancelled()
    }

    pub fn accept(self, value: T) {
        self.resolver.accept(value);
    }

    pub fn reject(self, err: E) {
        self.resolver.reject(err);
    }

    pub fn finish(self, res: Result<T, E>) {
        self.resolver.finish(res);
    }

    pub fn forget(self) {
        self.resolver.forget();
    }
}

// === PromiseResolver === //

pub struct PromiseResolver<T, E = PromiseCrashed>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    state: Option<Rc<State<T, E>>>,
}

impl<T, E> fmt::Debug for PromiseResolver<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromiseResolver").finish_non_exhaustive()
    }
}

impl<T, E> PromiseResolver<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    pub fn is_cancelled(&self) -> bool {
        self.state.as_ref().unwrap().cancelled.get()
    }

    pub fn accept(self, value: T) {
        self.finish(Ok(value));
    }

    pub fn reject(self, err: E) {
        self.finish(Err(err));
    }

    pub fn finish(mut self, res: Result<T, E>) {
        let state = self.state.take().unwrap();

        state.result.set(Some(res));

        if let Some(waker) = state.on_result_waker.take() {
            waker.wake();
        }
    }

    pub fn forget(mut self) {
        _ = self.state.take().unwrap();
    }
}

impl<T, E> Drop for PromiseResolver<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };

        let was_panic = thread::panicking();

        state
            .result
            .set(Some(Err(PromiseCrashed { was_panic }.into())));

        if let Some(waker) = state.on_result_waker.take() {
            waker.wake();
        }
    }
}

// === PromiseCancelledFuture === //

pub struct PromiseCancelledFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    state: Rc<State<T, E>>,
}

impl<T, E> fmt::Debug for PromiseCancelledFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromiseCancelledFuture")
            .finish_non_exhaustive()
    }
}

impl<T, E> PromiseCancelledFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    pub fn is_cancelled(&self) -> bool {
        self.state.cancelled.get()
    }
}

impl<T, E> Future for PromiseCancelledFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        if self.state.cancelled.get() {
            task::Poll::Ready(())
        } else {
            self.state.on_cancel_waker.set(Some(cx.waker().clone()));
            task::Poll::Pending
        }
    }
}

// === PromiseFuture === //

pub struct PromiseFuture<T, E = PromiseCrashed>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    state: Option<Rc<State<T, E>>>,
}

impl<T, E> fmt::Debug for PromiseFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromiseReceiver").finish_non_exhaustive()
    }
}

impl<T, E> Future for PromiseFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    type Output = Result<T, E>;

    fn poll(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> task::Poll<Self::Output> {
        let me = self.get_mut();
        let state = me.state.as_ref().unwrap();

        if let Some(res) = state.result.take() {
            me.state = None;
            task::Poll::Ready(res)
        } else {
            state.on_result_waker.set(Some(cx.waker().clone()));
            task::Poll::Pending
        }
    }
}

impl<T, E> Drop for PromiseFuture<T, E>
where
    T: 'static,
    E: 'static + From<PromiseCrashed>,
{
    fn drop(&mut self) {
        let Some(state) = self.state.take() else {
            return;
        };

        state.cancelled.set(true);

        if let Some(waker) = state.on_cancel_waker.take() {
            waker.wake();
        }
    }
}
