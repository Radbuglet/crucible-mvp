use std::{fmt, thread};

use futures::channel::oneshot;
use thiserror::Error;

// TODO: Consider making promises single-threaded.

// === Promise === //

#[derive(Debug, Clone, Error)]
#[error("{}", if self.was_panic { "worker thread panicked" } else { "promise dropped unexpectedly" })]
pub struct PromiseCancelled {
    pub was_panic: bool,
}

pub fn promise<T, E>() -> (Promise<T, E>, PromiseReceiver<T, E>)
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    let (tx, rx) = oneshot::channel();

    (Promise { tx: Some(tx) }, PromiseReceiver { rx })
}

pub struct Promise<T, E = PromiseCancelled>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    tx: Option<oneshot::Sender<Result<T, E>>>,
}

impl<T, E> fmt::Debug for Promise<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Promise").finish_non_exhaustive()
    }
}

impl<T, E> Promise<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    pub fn accept(self, value: T) {
        self.finish(Ok(value));
    }

    pub fn reject(self, err: E) {
        self.finish(Err(err));
    }

    pub fn finish(mut self, res: Result<T, E>) {
        _ = self.tx.take().unwrap().send(res);
    }

    pub fn forget(mut self) {
        self.tx.take();
    }
}

impl<T, E> Drop for Promise<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    fn drop(&mut self) {
        let Some(tx) = self.tx.take() else {
            return;
        };

        let was_panic = thread::panicking();

        _ = tx.send(Err(PromiseCancelled { was_panic }.into()));
    }
}

pub struct PromiseReceiver<T, E = PromiseCancelled>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    rx: oneshot::Receiver<Result<T, E>>,
}

impl<T, E> fmt::Debug for PromiseReceiver<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PromiseReceiver").finish_non_exhaustive()
    }
}

impl<T, E> PromiseReceiver<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    pub async fn recv(self) -> Result<T, E> {
        match self.rx.await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(PromiseCancelled { was_panic: false }.into()),
        }
    }
}
