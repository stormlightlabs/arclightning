//! Binding, accepting connections, handing requests to the read and
//! mutation threads, and shutting down.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::panic::{self, AssertUnwindSafe};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use axum::Router;
use axum::extract::{Request, State};
use axum::response::{IntoResponse, Response};
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use thiserror::Error;
use tokio::sync::{Semaphore, oneshot};

use crate::answer::{Answer, Reply, page};
use crate::gate::{Gate, Incoming, TOKEN_QUERY, new_token};
use crate::limits::{Limits, SHUTDOWN_GRACE, WriteDeadline, declares_too_much, read_body};
use crate::mutations::{Mutations, mutate};
use crate::routes::{Reads, Route, Write, dispatch};

/// How long the server pauses after a failed accept, such as when the
/// process is out of file descriptors.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// A failure to start or run the server.
#[derive(Debug, Error)]
pub enum ServeError {
    /// The port could not be bound.
    #[error("could not listen on 127.0.0.1:{port}: {source}")]
    Bind { port: u16, source: io::Error },
    /// The operating system's random source failed.
    #[error("could not generate the access token: {0}")]
    Token(getrandom::Error),
    /// The HTTP server could not start on the bound socket.
    #[error("could not start the HTTP server: {0}")]
    Start(io::Error),
}

/// A local server for the web app and its endpoints.
pub struct Server {
    listener: TcpListener,
    pub(crate) gate: Gate,
    /// The deadlines and the connection cap; the constants outside tests.
    pub(crate) limits: Limits,
}

impl Server {
    /// Bind `127.0.0.1` on `port`, or on an ephemeral port when `port` is
    /// `None`, and generate the token for this run.
    pub fn bind(port: Option<u16>) -> Result<Self, ServeError> {
        let requested = port.unwrap_or(0);
        let bind_error = |source| ServeError::Bind { port: requested, source };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, requested)).map_err(bind_error)?;
        listener.set_nonblocking(true).map_err(bind_error)?;
        let port = listener.local_addr().map_err(bind_error)?.port();
        let token = new_token()?;
        Ok(Self { listener, gate: Gate::new(token, port), limits: Limits::default() })
    }

    /// The port the server listens on.
    pub const fn port(&self) -> u16 {
        self.gate.port
    }

    /// The URL to open, with the token for the first page load.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?{TOKEN_QUERY}={}", self.gate.port, self.gate.token)
    }

    /// Serve requests until Ctrl-C, then let open connections finish for up
    /// to [`SHUTDOWN_GRACE`] and return. A second Ctrl-C ends the grace
    /// period, and this returns once the read or mutation already running
    /// finishes.
    ///
    /// Connections are served on a tokio runtime on its own thread. Reads run
    /// on this thread and admitted mutations on a mutation thread, each one at
    /// a time in the order their requests are ready. A read whose client has
    /// gone is dropped without running. After Ctrl-C, reads and mutations
    /// that have not started are answered 503 without running; one already
    /// running finishes before this returns, so a mutation's write and export
    /// stay together.
    pub fn run(self, reads: &dyn Reads, mutations: &(dyn Mutations + Sync)) -> Result<(), ServeError> {
        self.run_until(reads, mutations, || async {
            // Without a Ctrl-C handler, the default one ends the process.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
    }

    /// Serve requests as [`Server::run`] does, with `signal` in place of
    /// Ctrl-C. The server stops when the first future `signal` returns
    /// completes, and ends the grace period when the second one does.
    pub(crate) fn run_until<S: Future<Output = ()> + Send>(
        self, reads: &dyn Reads, mutations: &(dyn Mutations + Sync), signal: impl FnMut() -> S + Send,
    ) -> Result<(), ServeError> {
        let Self { listener, gate, limits } = self;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(ServeError::Start)?;
        let (reads_queue, read_jobs) = mpsc::channel::<Job<(Route, Vec<u8>)>>();
        let (writes_queue, write_jobs) = mpsc::channel::<Job<(Write, Vec<u8>)>>();
        let stopping = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Shared {
            gate,
            body_deadline: limits.body,
            stopping: Arc::clone(&stopping),
            reads: reads_queue,
            writes: writes_queue,
        });
        let reads_stopping = Arc::clone(&stopping);
        thread::scope(|scope| {
            scope.spawn(move || {
                for Job { work: (write, body), reply } in write_jobs {
                    let answer = if stopping.load(Ordering::SeqCst) {
                        Answer::json(Reply::stopping())
                    } else {
                        guarded(|| Answer::json(mutate(write, &body, mutations)))
                    };
                    // The client may have gone; the mutation ran either way.
                    let _ = reply.send(answer);
                }
            });
            // The runtime, and with it every handle on the job queues, drops
            // when this thread ends, which ends both job loops.
            let serving = scope.spawn(move || runtime.block_on(accept(listener, shared, limits, signal)));
            for Job { work: (route, body), reply } in read_jobs {
                // A client that closed its connection no longer waits for
                // the answer, so its read, such as a slow GraphQL query,
                // does not hold up the reads queued behind it.
                if reply.is_closed() {
                    continue;
                }
                let answer = if reads_stopping.load(Ordering::SeqCst) {
                    Answer::json(Reply::stopping())
                } else {
                    guarded(|| dispatch(route, &body, reads))
                };
                let _ = reply.send(answer);
            }
            serving.join().unwrap_or_else(|panic| panic::resume_unwind(panic))
        })
    }
}

/// What every connection's requests share: the checks and the job queues of
/// the read and mutation threads.
struct Shared {
    gate: Gate,
    body_deadline: Duration,
    /// Set when the server starts shutting down; reads and mutations that
    /// have not started no longer run.
    stopping: Arc<AtomicBool>,
    reads: mpsc::Sender<Job<(Route, Vec<u8>)>>,
    writes: mpsc::Sender<Job<(Write, Vec<u8>)>>,
}

/// Work for the read or mutation thread, and where to send its answer.
struct Job<T> {
    work: T,
    reply: oneshot::Sender<Answer>,
}

/// Accept connections on `listener` and serve each on its own task until
/// the first future from `signal` completes, then give open connections up
/// to [`SHUTDOWN_GRACE`] to finish, or until the second one completes.
///
/// At most `limits.connections` connections are open at once; the next one
/// is accepted when one closes. hyper closes a connection that takes longer
/// than `limits.header` to send a request head, and [`WriteDeadline`] one
/// whose client reads its replies slower than `limits.write_rate` once
/// `limits.write` of slack is spent.
async fn accept<S: Future<Output = ()>>(
    listener: TcpListener, shared: Arc<Shared>, limits: Limits, mut signal: impl FnMut() -> S,
) -> Result<(), ServeError> {
    let listener = tokio::net::TcpListener::from_std(listener).map_err(ServeError::Start)?;
    let stopping = Arc::clone(&shared.stopping);
    let service = TowerToHyperService::new(Router::new().fallback(handle).with_state(shared));
    let mut http = http1::Builder::new();
    http.timer(TokioTimer::new()).header_read_timeout(limits.header);
    let connections = Arc::new(Semaphore::new(limits.connections));
    let graceful = GracefulShutdown::new();
    let mut shutdown = pin!(signal());
    loop {
        let permit = tokio::select! {
            () = &mut shutdown => break,
            permit = Arc::clone(&connections).acquire_owned() => permit,
        };
        // The semaphore is never closed, so this always holds a permit.
        let Ok(permit) = permit else { break };
        let stream = tokio::select! {
            () = &mut shutdown => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(_) => {
                    // A failed accept, such as running out of file
                    // descriptors, passes; serving goes on.
                    tokio::time::sleep(ACCEPT_BACKOFF).await;
                    continue;
                }
            },
        };
        let stream = WriteDeadline::new(stream, limits.write, limits.write_rate);
        let connection = graceful.watch(http.serve_connection(TokioIo::new(stream), service.clone()));
        tokio::spawn(async move {
            // A client that goes away or times out is not the server's failure.
            let _ = connection.await;
            drop(permit);
        });
    }
    stopping.store(true, Ordering::SeqCst);
    let force = signal();
    tokio::select! {
        _ = tokio::time::timeout(SHUTDOWN_GRACE, graceful.shutdown()) => {}
        () = force => {}
    }
    Ok(())
}

/// Answer one request: refuse a declared body over the limit, check the
/// head, then read a mutation's body or pass a read to the read thread. A
/// GraphQL query is a read with a body, read under the mutations' limits.
async fn handle(State(shared): State<Arc<Shared>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    if declares_too_much(&parts.headers) {
        return Answer::closing(Reply::too_large()).into_response();
    }
    let answer = match shared.gate.admit(&Incoming::head(&parts)) {
        Err(answer) => answer,
        Ok(Route::Write(_)) if shared.stopping.load(Ordering::SeqCst) => Answer::closing(Reply::stopping()),
        Ok(Route::Write(write)) => match read_body(body, shared.body_deadline).await {
            Ok(body) => ask(&shared.writes, (write, body)).await,
            Err(answer) => answer,
        },
        Ok(Route::Graphql) => match read_body(body, shared.body_deadline).await {
            Ok(body) => ask(&shared.reads, (Route::Graphql, body)).await,
            Err(answer) => answer,
        },
        Ok(Route::Page) => page(),
        Ok(route) => ask(&shared.reads, (route, Vec::new())).await,
    };
    answer.into_response()
}

/// Send `work` to the thread behind `queue` and wait for its answer.
async fn ask<T>(queue: &mpsc::Sender<Job<T>>, work: T) -> Answer {
    let (reply, answer) = oneshot::channel();
    if queue.send(Job { work, reply }).is_err() {
        return Answer::json(Reply::stopping());
    }
    answer
        .await
        .unwrap_or_else(|_| Answer::json(Reply::internal("the request was dropped")))
}

/// Run `work`, answering 500 when it panics, so a panic in one read or
/// mutation leaves its thread serving the next.
fn guarded(work: impl FnOnce() -> Answer) -> Answer {
    panic::catch_unwind(AssertUnwindSafe(work))
        .unwrap_or_else(|_| Answer::json(Reply::internal("the operation failed unexpectedly")))
}
