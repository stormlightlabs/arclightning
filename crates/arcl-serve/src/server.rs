//! Binding, accepting connections, handing requests to the read and
//! mutation threads, and shutting down.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::panic::{self, AssertUnwindSafe};
use std::pin::pin;
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
    /// to [`SHUTDOWN_GRACE`] and return.
    ///
    /// Connections are served on a tokio runtime on its own thread. Reads run
    /// on this thread and admitted mutations on a mutation thread, each one at
    /// a time in the order their requests are ready.
    pub fn run(self, reads: &dyn Reads, mutations: &(dyn Mutations + Sync)) -> Result<(), ServeError> {
        self.run_until(reads, mutations, async {
            // Without a Ctrl-C handler, the default one ends the process.
            if tokio::signal::ctrl_c().await.is_err() {
                std::future::pending::<()>().await;
            }
        })
    }

    /// Serve requests as [`Server::run`] does until `shutdown` completes.
    pub(crate) fn run_until(
        self, reads: &dyn Reads, mutations: &(dyn Mutations + Sync), shutdown: impl Future<Output = ()> + Send,
    ) -> Result<(), ServeError> {
        let Self { listener, gate, limits } = self;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(ServeError::Start)?;
        let (reads_queue, read_jobs) = mpsc::channel::<Job<Route>>();
        let (writes_queue, write_jobs) = mpsc::channel::<Job<(Write, Vec<u8>)>>();
        let shared = Arc::new(Shared { gate, body_deadline: limits.body, reads: reads_queue, writes: writes_queue });
        thread::scope(|scope| {
            scope.spawn(move || {
                for Job { work: (write, body), reply } in write_jobs {
                    let answer = guarded(|| Answer::json(mutate(write, &body, mutations)));
                    // The client may have gone; the mutation ran either way.
                    let _ = reply.send(answer);
                }
            });
            // The runtime, and with it every handle on the job queues, drops
            // when this thread ends, which ends both job loops.
            let serving = scope.spawn(move || runtime.block_on(accept(listener, shared, limits, shutdown)));
            for Job { work: route, reply } in read_jobs {
                let _ = reply.send(guarded(|| dispatch(route, reads)));
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
    reads: mpsc::Sender<Job<Route>>,
    writes: mpsc::Sender<Job<(Write, Vec<u8>)>>,
}

/// Work for the read or mutation thread, and where to send its answer.
struct Job<T> {
    work: T,
    reply: oneshot::Sender<Answer>,
}

/// Accept connections on `listener` and serve each on its own task until
/// `shutdown` completes, then give open connections up to
/// [`SHUTDOWN_GRACE`] to finish.
///
/// At most `limits.connections` connections are open at once; the next one
/// is accepted when one closes. hyper closes a connection that takes longer
/// than `limits.header` to send a request head, and [`WriteDeadline`] one
/// whose writes wait longer than `limits.write` without progress.
async fn accept(
    listener: TcpListener, shared: Arc<Shared>, limits: Limits, shutdown: impl Future<Output = ()>,
) -> Result<(), ServeError> {
    let listener = tokio::net::TcpListener::from_std(listener).map_err(ServeError::Start)?;
    let service = TowerToHyperService::new(Router::new().fallback(handle).with_state(shared));
    let mut http = http1::Builder::new();
    http.timer(TokioTimer::new()).header_read_timeout(limits.header);
    let connections = Arc::new(Semaphore::new(limits.connections));
    let graceful = GracefulShutdown::new();
    let mut shutdown = pin!(shutdown);
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
        let stream = WriteDeadline::new(stream, limits.write);
        let connection = graceful.watch(http.serve_connection(TokioIo::new(stream), service.clone()));
        tokio::spawn(async move {
            // A client that goes away or times out is not the server's failure.
            let _ = connection.await;
            drop(permit);
        });
    }
    let _ = tokio::time::timeout(SHUTDOWN_GRACE, graceful.shutdown()).await;
    Ok(())
}

/// Answer one request: refuse a declared body over the limit, check the
/// head, then read a mutation's body or pass a read to the read thread.
async fn handle(State(shared): State<Arc<Shared>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    if declares_too_much(&parts.headers) {
        return Answer::closing(Reply::too_large()).into_response();
    }
    let answer = match shared.gate.admit(&Incoming::head(&parts)) {
        Err(answer) => answer,
        Ok(Route::Write(write)) => match read_body(body, shared.body_deadline).await {
            Ok(body) => ask(&shared.writes, (write, body)).await,
            Err(answer) => answer,
        },
        Ok(Route::Page) => page(),
        Ok(route) => ask(&shared.reads, route).await,
    };
    answer.into_response()
}

/// Send `work` to the thread behind `queue` and wait for its answer.
async fn ask<T>(queue: &mpsc::Sender<Job<T>>, work: T) -> Answer {
    let (reply, answer) = oneshot::channel();
    if queue.send(Job { work, reply }).is_err() {
        return Answer::json(Reply::internal("the server is stopping"));
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
