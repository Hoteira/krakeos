use crate::fs::File;
use crate::io::{AsyncRead, AsyncWrite, Read, Write};
use crate::os::set_nonblocking;
use core::pin::Pin;
use core::task::{Context, Poll};

pub struct AsyncFile {
    inner: File,
}

impl AsyncFile {
    pub fn new(file: File) -> Self {
        set_nonblocking(file.as_raw_fd(), true);
        AsyncFile { inner: file }
    }
}

impl AsyncRead for AsyncFile {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<crate::io::Result<usize>> {
        match self.inner.read(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.is_would_block() => {
                crate::os::REACTOR.lock().read_waiters.insert(self.inner.as_raw_fd() as i32, cx.waker().clone());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }
}

impl AsyncWrite for AsyncFile {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<crate::io::Result<usize>> {
        match self.inner.write(buf) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(e) if e.is_would_block() => {
                crate::os::REACTOR.lock().write_waiters.insert(self.inner.as_raw_fd() as i32, cx.waker().clone());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<crate::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}
