use std::{fmt, thread};

use smallbox::{SmallBox, smallbox};
use thiserror::Error;

// === Promise === //

#[derive(Debug, Clone, Error)]
#[error("{}", if self.was_panic { "worker thread panicked" } else { "promise dropped unexpectedly" })]
pub struct PromiseCancelled {
    pub was_panic: bool,
}

pub struct Promise<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    #[expect(clippy::type_complexity)]
    callback: Option<SmallBox<dyn Send + FnMut(Result<T, E>), [u64; 2]>>,
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
    pub fn new(callback: impl 'static + Send + FnOnce(Result<T, E>)) -> Self {
        let mut callback = Some(callback);

        Self {
            callback: Some(smallbox!(move |value| callback.take().unwrap()(value))),
        }
    }

    pub fn accept(self, value: T) {
        self.finish(Ok(value));
    }

    pub fn reject(self, err: E) {
        self.finish(Err(err));
    }

    pub fn finish(mut self, res: Result<T, E>) {
        self.callback.take().unwrap()(res);
    }

    pub fn forget(mut self) {
        self.callback.take();
    }
}

impl<T, E> Drop for Promise<T, E>
where
    T: 'static + Send,
    E: 'static + Send + From<PromiseCancelled>,
{
    fn drop(&mut self) {
        let Some(mut callback) = self.callback.take() else {
            return;
        };

        let was_panic = thread::panicking();

        callback(Err(PromiseCancelled { was_panic }.into()));
    }
}
