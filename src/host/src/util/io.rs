use std::{io::Write, pin::Pin, task};

use tokio::io::{self, AsyncWrite};

#[derive(Debug)]
pub struct SyncWriteAsAsync<W>(pub W);

impl<W> Unpin for SyncWriteAsAsync<W> {}

impl<W: Write> AsyncWrite for SyncWriteAsAsync<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
        buf: &[u8],
    ) -> task::Poll<Result<usize, io::Error>> {
        // N.B. we don't need to wake anything if we immediately resolve.
        let _ = cx;

        task::Poll::Ready(self.get_mut().0.write(buf))
    }

    fn poll_flush(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Result<(), io::Error>> {
        // N.B. we don't need to wake anything if we immediately resolve.
        let _ = cx;

        task::Poll::Ready(self.get_mut().0.flush())
    }

    fn poll_shutdown(
        self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Result<(), io::Error>> {
        // N.B. we don't need to wake anything if we immediately resolve.
        let _ = cx;

        task::Poll::Ready(Ok(()))
    }
}
