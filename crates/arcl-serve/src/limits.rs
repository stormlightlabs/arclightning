//! Deadlines, the body limit, the connection cap, and the connection
//! wrapper that times out clients that read their replies too slowly.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::http::HeaderMap;
use axum::http::header::CONTENT_LENGTH;
use http_body_util::{BodyExt, LengthLimitError, Limited};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::time::{Instant, Sleep};

use crate::answer::{Answer, Reply};

/// How long the server waits for all of a mutation's body before it answers
/// 408 and closes the connection.
pub const BODY_DEADLINE: Duration = Duration::from_secs(5);

/// How long a connection may take to send a request head, counted from when
/// the server starts waiting for one. A connection that idles longer between
/// requests is closed too.
pub const HEADER_DEADLINE: Duration = Duration::from_secs(5);

/// The most time a connection may have banked for waiting on its client to
/// read replies, and the time it starts with. Waiting spends it, each byte
/// the client takes earns it back at [`MIN_WRITE_RATE`], and the server
/// closes the connection when it runs out. No single wait passes it.
pub const WRITE_DEADLINE: Duration = Duration::from_secs(5);

/// The slowest rate, in bytes per second, at which a client may read its
/// replies while they wait on it. A client that reads slower uses up
/// [`WRITE_DEADLINE`] and loses its connection, however steadily it reads.
pub const MIN_WRITE_RATE: u64 = 64 * 1024;

/// The most connections open at once. Past it, new connections wait in the
/// listen queue until one closes.
pub const MAX_CONNECTIONS: usize = 128;

/// How long Ctrl-C waits for open connections to finish before the server
/// returns.
pub const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// The largest request body a mutation accepts, in bytes.
pub(crate) const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The deadlines and connection cap a [`Server`](crate::Server) runs with.
#[derive(Clone, Copy)]
pub(crate) struct Limits {
    /// How long to wait for a mutation's body.
    pub(crate) body: Duration,
    /// How long to wait for a request head.
    pub(crate) header: Duration,
    /// The most connections open at once.
    pub(crate) connections: usize,
    /// The most waiting on the client's reads a connection may bank.
    pub(crate) write: Duration,
    /// The bytes per second a client's reads earn back.
    pub(crate) write_rate: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            body: BODY_DEADLINE,
            header: HEADER_DEADLINE,
            connections: MAX_CONNECTIONS,
            write: WRITE_DEADLINE,
            write_rate: MIN_WRITE_RATE,
        }
    }
}

/// A connection whose writes fail with [`io::ErrorKind::TimedOut`] once the
/// client reads too slowly.
///
/// The connection holds a budget of time to spend waiting on the client,
/// starting at `limit`. A write that waits spends it; every byte written
/// earns back `1 / rate` seconds, up to `limit`. Bytes the kernel buffers
/// earn nothing past `limit`, so a client cannot bank time by letting the
/// buffers fill, and progress alone does not keep a slow client connected.
pub(crate) struct WriteDeadline<T> {
    inner: T,
    limit: Duration,
    rate: u64,
    budget: Duration,
    /// When the current wait began, and the timer that ends it when the
    /// budget runs out. Cleared when a write or flush completes.
    waiting: Option<(Instant, Pin<Box<Sleep>>)>,
}

impl<T> WriteDeadline<T> {
    pub(crate) fn new(inner: T, limit: Duration, rate: u64) -> Self {
        Self { inner, limit, rate: rate.max(1), budget: limit, waiting: None }
    }

    /// Settle the budget when `poll` completed, having written `written`
    /// bytes; start the timer when `poll` waits; fail once the budget is
    /// spent.
    fn timed<R>(
        &mut self, cx: &mut Context<'_>, poll: Poll<io::Result<R>>, written: impl FnOnce(&R) -> usize,
    ) -> Poll<io::Result<R>> {
        if let Poll::Ready(result) = &poll {
            if let Some((since, _)) = self.waiting.take() {
                self.budget = self.budget.saturating_sub(since.elapsed());
            }
            let bytes = result.as_ref().map_or(0, written);
            self.budget = self.budget.saturating_add(self.earned(bytes)).min(self.limit);
            return poll;
        }
        let budget = self.budget;
        let (_, timer) = self
            .waiting
            .get_or_insert_with(|| (Instant::now(), Box::pin(tokio::time::sleep(budget))));
        match timer.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the client read its replies too slowly",
            ))),
            Poll::Pending => Poll::Pending,
        }
    }

    /// The waiting time `bytes` written earn back.
    fn earned(&self, bytes: usize) -> Duration {
        let nanos = u128::try_from(bytes).unwrap_or(u128::MAX).saturating_mul(1_000_000_000) / u128::from(self.rate);
        Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for WriteDeadline<T> {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for WriteDeadline<T> {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        let poll = Pin::new(&mut self.inner).poll_write(cx, buf);
        self.timed(cx, poll, |written| *written)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let poll = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        self.timed(cx, poll, |written| *written)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let poll = Pin::new(&mut self.inner).poll_flush(cx);
        self.timed(cx, poll, |()| 0)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let poll = Pin::new(&mut self.inner).poll_shutdown(cx);
        self.timed(cx, poll, |()| 0)
    }
}

/// Whether `headers` declare a body over [`MAX_BODY_BYTES`]. hyper refuses a
/// malformed `Content-Length` before the request gets here.
pub(crate) fn declares_too_much(headers: &HeaderMap) -> bool {
    let limit = u64::try_from(MAX_BODY_BYTES).unwrap_or(u64::MAX);
    headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .is_some_and(|length| length > limit)
}

/// Read all of `body`, up to [`MAX_BODY_BYTES`], within `deadline`, or the
/// refusal: 408 when the deadline passes, 413 when the body runs over the
/// limit, and 400 when the read fails. Each refusal leaves the body unread,
/// so it carries `Connection: close` and closes the connection.
pub(crate) async fn read_body(body: Body, deadline: Duration) -> Result<Vec<u8>, Answer> {
    match tokio::time::timeout(deadline, Limited::new(body, MAX_BODY_BYTES).collect()).await {
        Err(_) => Err(Answer::closing(Reply::timed_out())),
        Ok(Ok(collected)) => Ok(collected.to_bytes().to_vec()),
        Ok(Err(error)) if error.is::<LengthLimitError>() => Err(Answer::closing(Reply::too_large())),
        Ok(Err(_)) => Err(Answer::closing(Reply::invalid_request(
            "the request body could not be read",
        ))),
    }
}
