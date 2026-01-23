use crate::io::Result;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

pub trait AsyncRead {
    fn poll_read(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<Result<usize>>;
}

pub trait AsyncWrite {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<Result<usize>>;
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>>;
}

pub async fn read_ext<R: AsyncRead + Unpin>(reader: &mut R, buf: &mut [u8]) -> Result<usize> {
    struct ReadFuture<'a, R: AsyncRead + Unpin> {
        reader: &'a mut R,
        buf: &'a mut [u8],
    }

    impl<'a, R: AsyncRead + Unpin> Future for ReadFuture<'a, R> {
        type Output = Result<usize>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            Pin::new(&mut *this.reader).poll_read(cx, this.buf)
        }
    }

    ReadFuture { reader, buf }.await
}

pub async fn write_ext<W: AsyncWrite + Unpin>(writer: &mut W, buf: &[u8]) -> Result<usize> {
    struct WriteFuture<'a, W: AsyncWrite + Unpin> {
        writer: &'a mut W,
        buf: &'a [u8],
    }

    impl<'a, W: AsyncWrite + Unpin> Future for WriteFuture<'a, W> {
        type Output = Result<usize>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            let this = self.get_mut();
            Pin::new(&mut *this.writer).poll_write(cx, this.buf)
        }
    }

    WriteFuture { writer, buf }.await
}
