//! Deadlines, the body limit, the connection cap, and the connection
//! wrapper that times out stalled writes.

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
use tokio::time::Sleep;

use crate::answer::{Answer, Reply};

/// How long the server waits for all of a mutation's body before it answers
/// 408 and closes the connection.
pub const BODY_DEADLINE: Duration = Duration::from_secs(5);

/// How long a connection may take to send a request head, counted from when
/// the server starts waiting for one. A connection that idles longer between
/// requests is closed too.
pub const HEADER_DEADLINE: Duration = Duration::from_secs(5);

/// How long a write to a client may wait without progress before the server
/// closes the connection, so a client that never reads its replies cannot
/// hold a connection open.
pub const WRITE_DEADLINE: Duration = Duration::from_secs(5);

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
    /// How long a write to the client may wait without progress.
    pub(crate) write: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self { body: BODY_DEADLINE, header: HEADER_DEADLINE, connections: MAX_CONNECTIONS, write: WRITE_DEADLINE }
    }
}

/// A connection whose writes fail with [`io::ErrorKind::TimedOut`] once one
/// has waited `limit` without progress, so a client that stops reading its
/// replies loses its connection instead of holding it open.
pub(crate) struct WriteDeadline<T> {
    inner: T,
    limit: Duration,
    /// Runs while a write waits; any progress clears it.
    stall: Option<Pin<Box<Sleep>>>,
}

impl<T> WriteDeadline<T> {
    pub(crate) fn new(inner: T, limit: Duration) -> Self {
        Self { inner, limit, stall: None }
    }

    /// Clear the timer when `poll` made progress, start it when `poll` waits,
    /// and fail once it has run out.
    fn timed<R>(&mut self, cx: &mut Context<'_>, poll: Poll<io::Result<R>>) -> Poll<io::Result<R>> {
        if poll.is_ready() {
            self.stall = None;
            return poll;
        }
        let limit = self.limit;
        let stall = self.stall.get_or_insert_with(|| Box::pin(tokio::time::sleep(limit)));
        match stall.as_mut().poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "the client stopped reading",
            ))),
            Poll::Pending => Poll::Pending,
        }
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
        self.timed(cx, poll)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>, cx: &mut Context<'_>, bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        let poll = Pin::new(&mut self.inner).poll_write_vectored(cx, bufs);
        self.timed(cx, poll)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let poll = Pin::new(&mut self.inner).poll_flush(cx);
        self.timed(cx, poll)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let poll = Pin::new(&mut self.inner).poll_shutdown(cx);
        self.timed(cx, poll)
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
/// limit, and 400 when the read fails. hyper closes the connection after a
/// response that leaves the body unread.
pub(crate) async fn read_body(body: Body, deadline: Duration) -> Result<Vec<u8>, Answer> {
    match tokio::time::timeout(deadline, Limited::new(body, MAX_BODY_BYTES).collect()).await {
        Err(_) => Err(Answer::json(Reply::timed_out())),
        Ok(Ok(collected)) => Ok(collected.to_bytes().to_vec()),
        Ok(Err(error)) if error.is::<LengthLimitError>() => Err(Answer::json(Reply::too_large())),
        Ok(Err(_)) => Err(Answer::json(Reply::invalid_request(
            "the request body could not be read",
        ))),
    }
}
