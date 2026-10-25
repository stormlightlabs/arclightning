//! Local HTTP adapter for the Arc Lightning web app.
//!
//! [`Server`] binds `127.0.0.1`, checks every request, serves the web app
//! page embedded from `apps/web/build/index.html` for any path outside
//! `/api`, routes the read endpoints to a [`Reads`] implementation and the
//! mutations to a [`Mutations`] implementation. Both answer with the
//! CLI's JSON envelopes.
//!
//! Every request needs a `Host` header naming the server and the per-run
//! token in the [`TOKEN_HEADER`] header. The one exception is the first page
//! load, which carries the token in the [`TOKEN_QUERY`] query parameter of the
//! printed URL; the server accepts a query token once, and never on an API
//! path. A mutation also needs an `Origin` header naming the server. Reads
//! take GET and mutations take POST; any other method is refused, so GET never
//! changes state. Responses carry no CORS headers.
//!
//! The server runs axum on hyper and a single-threaded tokio runtime. It
//! checks a request's head before it reads any of the body, and reads a body
//! only for a mutation that passed every check. A request that declares a
//! body over 1 MiB gets 413 before any check, and its body is never read. A
//! mutation's body that does not arrive within [`BODY_DEADLINE`] gets 408. A
//! connection that does not send a request head within [`HEADER_DEADLINE`] is
//! closed, idle keep-alive connections included, as is one whose replies wait
//! [`WRITE_DEADLINE`] for the client to read them. A refusal before the token
//! is accepted (a bad `Host` or token, a bad request target, or 413) carries
//! `Connection: close` and closes the connection. At most
//! [`MAX_CONNECTIONS`] are open at once; past that, new connections wait to
//! be accepted.
//!
//! [`Reads`] and [`Mutations`] stay synchronous. Reads run one at a time on
//! the thread that called [`Server::run`], and mutations one at a time on
//! their own thread, so a mutation that waits for a lock never holds up reads
//! and page loads. Ctrl-C stops accepting connections, lets open ones finish
//! for up to [`SHUTDOWN_GRACE`], and returns.
//!
//! The page's Content-Security-Policy runs only the app's own inline script,
//! by hash, and lets it load images and fonts from itself or `data:` URLs and
//! connect only to this server. API responses allow nothing to load.
//!
//! | Request                          | CLI equivalent           |
//! | -------------------------------- | ------------------------ |
//! | `GET /api/records`               | `arcl list`              |
//! | `GET /api/records/{id}`          | `arcl show <id>`         |
//! | `GET /api/ready`                 | `arcl ready`             |
//! | `GET /api/tasks/{id}/explain`    | `arcl explain <task-id>` |
//! | `GET /api/tasks/{id}/context`    | `arcl context <task-id>` |
//! | `GET /api/tree`                  | `arcl tree`              |
//! | `GET /api/work`                  | none; see below          |
//! | `POST /api/tasks`                | `arcl task create`       |
//! | `POST /api/tasks/{id}/update`    | `arcl task update`       |
//! | `POST /api/tasks/{id}/start`     | `arcl task start`        |
//! | `POST /api/tasks/{id}/park`      | `arcl task park`         |
//! | `POST /api/tasks/{id}/unpark`    | `arcl task unpark`       |
//! | `POST /api/tasks/{id}/handoff`   | `arcl task handoff`      |
//! | `POST /api/tasks/{id}/complete`  | `arcl task complete`     |
//! | `POST /api/captures`             | `arcl capture create`    |
//! | `POST /api/captures/{id}/promote`| `arcl capture promote`   |
//! | `POST /api/captures/{id}/discard`| `arcl capture discard`   |
//! | `POST /api/specs`                | `arcl spec create`       |
//! | `POST /api/plans`                | `arcl plan create`       |
//! | `POST /api/plans/{id}/apply`     | `arcl plan apply`        |
//!
//! `/api/records` and `/api/ready` take the CLI's filter flags as query
//! parameters, such as `?kind=task&status=pending&status=parked`.
//!
//! `/api/tree` answers `arcl tree --json` for every record and takes no query
//! parameters.
//!
//! `/api/work` answers the web app's Work list in one request: every task
//! with the readiness `arcl explain` reports and its effective phase ID, and
//! every phase, as `{"format_version": 2, "data": {"tasks": [{"task",
//! "readiness", "phase_id"}], "phases": [...]}}`. It takes no query
//! parameters.
//!
//! A mutation takes a JSON object of the command's options, such as
//! `{"evidence": "Delivered by #41."}`; see [`NewTask`], [`TaskEdit`],
//! [`TaskStart`], [`Handoff`], [`Completion`], [`NewCapture`],
//! [`CapturePromotion`], [`NewSpec`], [`NewPlan`] and [`PlanInput`]. A plan
//! applies from the TOML document `arcl plan apply --file` reads, sent as the
//! `document` string.

use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::panic::{self, AssertUnwindSafe};
use std::pin::{Pin, pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, mpsc};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::header::CONTENT_LENGTH;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use http_body_util::{BodyExt, LengthLimitError, Limited};
use hyper::server::conn::http1;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use hyper_util::service::TowerToHyperService;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::{Semaphore, oneshot};
use tokio::time::Sleep;

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The request header that carries the per-run token.
pub const TOKEN_HEADER: &str = "X-Arcl-Token";

/// The query parameter that carries the token on the first page load.
pub const TOKEN_QUERY: &str = "token";

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

/// How long the server pauses after a failed accept, such as when the
/// process is out of file descriptors.
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// The `Retry-After` value, in seconds, of a [`Reply::busy`] answer.
const RETRY_AFTER_SECONDS: &str = "1";

/// The number of random bytes in a token, which is printed as hex.
const TOKEN_BYTES: usize = 32;

/// The largest request body a mutation accepts, in bytes.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The page served when `apps/web` was not built before `arcl` was compiled.
const NOT_BUILT_PAGE: &str = include_str!("not_built.html");

/// The Content-Security-Policy of every API response: a JSON body loads nothing.
const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// The Content-Security-Policy of the page. Scripts run only when their hash
/// is one of the built page's inline scripts. Styles may be inline, since
/// Svelte and CodeMirror set style attributes and elements at run time.
static PAGE_CSP: LazyLock<String> = LazyLock::new(|| {
    format!(
        "default-src 'self'; script-src 'self' {INDEX_SCRIPT_HASHES}; style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; font-src 'self' data:; connect-src 'self'; object-src 'none'; \
         frame-ancestors 'none'; base-uri 'none'; form-action 'self'"
    )
});

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

/// Where a request that passed the checks goes.
#[derive(Debug, Eq, PartialEq)]
enum Route {
    /// `arcl list` with these filters.
    List(Vec<(String, String)>),
    /// `arcl show <id>`.
    Show(String),
    /// `arcl ready` with these filters.
    Ready(Vec<(String, String)>),
    /// `arcl explain <task-id>`.
    Explain(String),
    /// `arcl context <task-id>`.
    Context(String),
    /// `arcl tree`, of every record. Holds whether the request carried query
    /// parameters, which it refuses.
    Tree { has_query: bool },
    /// Every task with its readiness and phase, and every phase. Holds
    /// whether the request carried query parameters, which it refuses.
    Work { has_query: bool },
    /// A mutation, which needs POST and a matching `Origin`.
    Write(Write),
    /// An `/api` path with no endpoint.
    UnknownApi,
    /// A web app page. The app routes on the client, so every page is
    /// `index.html`.
    Page,
}

/// A mutation and the record it names.
#[derive(Debug, Eq, PartialEq)]
enum Write {
    /// `arcl task create`.
    Create,
    /// `arcl task update <id>`.
    Update(String),
    /// `arcl task start <id>`.
    Start(String),
    /// `arcl task park <id>`.
    Park(String),
    /// `arcl task unpark <id>`.
    Unpark(String),
    /// `arcl task handoff <id>`.
    Handoff(String),
    /// `arcl task complete <id>`.
    Complete(String),
    /// `arcl capture create`.
    CreateCapture,
    /// `arcl capture promote <id>`.
    PromoteCapture(String),
    /// `arcl capture discard <id>`.
    DiscardCapture(String),
    /// `arcl spec create`.
    CreateSpec,
    /// `arcl plan create`.
    CreatePlan,
    /// `arcl plan apply <id>`.
    ApplyPlan(String),
}

/// The JSON answer to an API request: an HTTP status and a body.
#[derive(Debug, Eq, PartialEq)]
pub struct Reply {
    status: u16,
    body: String,
    /// Whether the client may retry soon, sent as a `Retry-After` header.
    retry: bool,
}

impl Reply {
    /// A successful read whose body is the CLI's JSON envelope.
    pub fn success(body: String) -> Self {
        Self { status: 200, body, retry: false }
    }

    /// A failed read. `exit_code` is the CLI's exit code for the failure and
    /// `body` its JSON error envelope.
    pub fn failure(exit_code: u8, body: String) -> Self {
        let status = match exit_code {
            2 | 3 => 400,
            4 => 409,
            5 => 404,
            _ => 500,
        };
        Self { status, body, retry: false }
    }

    /// A failure because another process holds something the operation
    /// needs, such as the project's sync lock. `body` is the CLI's JSON error
    /// envelope. The status is 503, with a `Retry-After` header.
    pub fn busy(body: String) -> Self {
        Self { status: 503, body, retry: true }
    }

    /// A malformed request, with the envelope the CLI prints for a usage
    /// error under `--json`.
    pub fn invalid_request(message: &str) -> Self {
        let body = serde_json::json!({
            "format_version": 1,
            "error": { "code": "invalid_request", "message": message, "exit_code": 2 },
        });
        Self { status: 400, body: body.to_string(), retry: false }
    }

    /// The HTTP status code.
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// The JSON body.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// A mutation whose body did not arrive within [`BODY_DEADLINE`].
    fn timed_out() -> Self {
        Self::refusal(408, "request_timeout", "the request body did not arrive in time")
    }

    /// A request whose body is, or is declared, over [`MAX_BODY_BYTES`].
    fn too_large() -> Self {
        Self::refusal(413, "payload_too_large", "the request body is over 1 MiB")
    }

    /// A failure inside the server, such as a worker that panicked.
    fn internal(message: &str) -> Self {
        Self::refusal(500, "internal_error", message)
    }

    /// A request the server refused before it reached the application.
    fn refusal(status: u16, code: &str, message: &str) -> Self {
        let body = serde_json::json!({
            "format_version": 1,
            "error": { "code": code, "message": message },
        });
        Self { status, body: body.to_string(), retry: false }
    }
}

/// The options of `arcl task create`, as the body of `POST /api/tasks`.
/// `priority` defaults to `normal`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewTask {
    /// The task title.
    pub title: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// The task priority.
    #[serde(default)]
    pub priority: Option<String>,
    /// The parent task's ID.
    #[serde(default)]
    pub parent: Option<String>,
    /// The spec's ID.
    #[serde(default)]
    pub spec: Option<String>,
    /// The plan's ID.
    #[serde(default)]
    pub plan: Option<String>,
    /// The phase's ID.
    #[serde(default)]
    pub phase: Option<String>,
    /// The IDs of the tasks that must complete first.
    #[serde(default)]
    pub blocked_by: Vec<String>,
}

/// The options of `arcl task update <id>`. A field left out keeps its value.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskEdit {
    /// The new title.
    #[serde(default)]
    pub title: Option<String>,
    /// The new Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// The new priority.
    #[serde(default)]
    pub priority: Option<String>,
}

/// The options of `arcl task start <id>`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct TaskStart {
    /// Start the task even when it is not ready.
    #[serde(default)]
    pub force: bool,
}

/// The options of `arcl task handoff <id>`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Handoff {
    /// The Markdown resume note. The CLI refuses a handoff without one.
    #[serde(default)]
    pub note: Option<String>,
}

/// The options of `arcl task complete <id>`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    /// Markdown completion evidence.
    #[serde(default)]
    pub evidence: Option<String>,
    /// Complete only this task even when descendants are open.
    #[serde(default)]
    pub allow_open_children: bool,
    /// Also complete every pending and in-progress descendant.
    #[serde(default)]
    pub with_subtasks: bool,
}

/// The options of `arcl capture create`, as the body of `POST /api/captures`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewCapture {
    /// The capture title.
    pub title: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
}

/// The options of `arcl capture promote <id>`, as the body of
/// `POST /api/captures/{id}/promote`. A title or body left out is the
/// capture's. `acceptance_criteria` applies to a spec; `spec`, `plan`,
/// `phase`, `parent` and `priority` apply to a task, whose priority defaults
/// to `normal`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct CapturePromotion {
    /// What the capture becomes: `task`, `spec` or `note`.
    pub to: String,
    /// The new record's title.
    #[serde(default)]
    pub title: Option<String>,
    /// The new record's Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// A spec's Markdown acceptance criteria.
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
    /// A task's spec ID.
    #[serde(default)]
    pub spec: Option<String>,
    /// A task's plan ID.
    #[serde(default)]
    pub plan: Option<String>,
    /// A task's phase ID.
    #[serde(default)]
    pub phase: Option<String>,
    /// A task's parent task ID.
    #[serde(default)]
    pub parent: Option<String>,
    /// A task's priority.
    #[serde(default)]
    pub priority: Option<String>,
}

/// The options of `arcl spec create`, as the body of `POST /api/specs`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewSpec {
    /// The spec title.
    pub title: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
    /// The Markdown acceptance criteria.
    #[serde(default)]
    pub acceptance_criteria: Option<String>,
}

/// The options of `arcl plan create`, as the body of `POST /api/plans`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct NewPlan {
    /// The plan title.
    pub title: String,
    /// The ID of the spec that owns the plan.
    pub spec: String,
    /// The Markdown body.
    #[serde(default)]
    pub body: Option<String>,
}

/// The input of `arcl plan apply <id>`, as the body of
/// `POST /api/plans/{id}/apply`.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PlanInput {
    /// The structured plan as TOML, as `--file` holds it.
    pub document: String,
}

/// A local server for the web app and its endpoints.
pub struct Server {
    listener: TcpListener,
    gate: Gate,
    /// The deadlines and the connection cap; the constants outside tests.
    limits: Limits,
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
    fn run_until(
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

/// The body of a mutation that takes no options: an empty object.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoOptions {}

/// The deadlines and connection cap a [`Server`] runs with.
#[derive(Clone, Copy)]
struct Limits {
    /// How long to wait for a mutation's body.
    body: Duration,
    /// How long to wait for a request head.
    header: Duration,
    /// The most connections open at once.
    connections: usize,
    /// How long a write to the client may wait without progress.
    write: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self { body: BODY_DEADLINE, header: HEADER_DEADLINE, connections: MAX_CONNECTIONS, write: WRITE_DEADLINE }
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

/// The parts of a request the checks and routes read.
struct Incoming {
    method: String,
    url: String,
    host: Option<String>,
    origin: Option<String>,
    token: Option<String>,
}

impl Incoming {
    /// The head of a request, with no body read. A header that is not
    /// visible ASCII counts as missing.
    fn head(parts: &Parts) -> Self {
        let header = |name: &str| {
            parts
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        Self {
            method: parts.method.as_str().to_owned(),
            url: parts
                .uri
                .path_and_query()
                .map_or_else(|| "/".to_owned(), |target| target.as_str().to_owned()),
            host: header("Host"),
            origin: header("Origin"),
            token: header(TOKEN_HEADER),
        }
    }
}

/// A response before it is written to the socket.
struct Answer {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    /// The method to name in an `Allow` header, on a 405.
    allow: Option<&'static str>,
    /// The `Retry-After` header's value, if any.
    retry_after: Option<&'static str>,
    /// The Content-Security-Policy header's value.
    csp: &'static str,
    /// Whether to send `Connection: close` and close the connection after
    /// this response.
    close: bool,
}

impl Answer {
    /// The headers every response carries, with no CORS headers among them.
    fn headers(&self) -> Vec<(&'static str, &'static str)> {
        let mut headers = vec![
            ("Content-Type", self.content_type),
            ("Cache-Control", "no-store"),
            ("X-Content-Type-Options", "nosniff"),
            ("Referrer-Policy", "no-referrer"),
            ("X-Frame-Options", "DENY"),
            ("Content-Security-Policy", self.csp),
        ];
        if let Some(method) = self.allow {
            headers.push(("Allow", method));
        }
        if let Some(seconds) = self.retry_after {
            headers.push(("Retry-After", seconds));
        }
        if self.close {
            headers.push(("Connection", "close"));
        }
        headers
    }

    fn json(reply: Reply) -> Self {
        Self {
            status: reply.status,
            content_type: "application/json",
            body: reply.body.into_bytes(),
            allow: None,
            retry_after: reply.retry.then_some(RETRY_AFTER_SECONDS),
            csp: API_CSP,
            close: false,
        }
    }

    /// The answer to a request refused before its token was accepted, which
    /// closes the connection so a client without the token gets one reply
    /// per connection.
    fn closing(reply: Reply) -> Self {
        Self { close: true, ..Self::json(reply) }
    }
}

impl IntoResponse for Answer {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = Response::builder().status(status);
        for (name, value) in self.headers() {
            response = response.header(name, value);
        }
        // Every header is a constant or the page's CSP, so this always builds.
        response
            .body(Body::from(self.body))
            .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
    }
}

/// A connection whose writes fail with [`io::ErrorKind::TimedOut`] once one
/// has waited `limit` without progress, so a client that stops reading its
/// replies loses its connection instead of holding it open.
struct WriteDeadline<T> {
    inner: T,
    limit: Duration,
    /// Runs while a write waits; any progress clears it.
    stall: Option<Pin<Box<Sleep>>>,
}

impl<T> WriteDeadline<T> {
    fn new(inner: T, limit: Duration) -> Self {
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

/// The checks every request passes before it is routed.
struct Gate {
    token: String,
    port: u16,
    /// Whether a page load has used the query token.
    first_load_used: AtomicBool,
}

impl Gate {
    fn new(token: String, port: u16) -> Self {
        Self { token, port, first_load_used: AtomicBool::new(false) }
    }

    /// Check the head of `incoming` and route it, or answer with the refusal.
    /// The body plays no part, so a refused request is never read. A refusal
    /// before the token is accepted closes the connection.
    fn admit(&self, incoming: &Incoming) -> Result<Route, Answer> {
        let (path, query) =
            split_url(&incoming.url).map_err(|message| Answer::closing(Reply::invalid_request(&message)))?;
        let query_token = token_in(&query);
        let route = route(&path, query);
        if self
            .authorize(incoming, &route, query_token.as_deref())
            .map_err(Answer::closing)?
        {
            return Ok(route);
        }
        if let Err(reply) = Self::permit(incoming, &route) {
            let allow = (reply.status == 405).then_some(if matches!(route, Route::Write(_)) { "POST" } else { "GET" });
            return Err(Answer { allow, ..Answer::json(reply) });
        }
        Ok(route)
    }

    /// Refuse a request with the wrong `Host` or without the token. Answers
    /// `true` for the first page load, which needs no further checks.
    fn authorize(&self, incoming: &Incoming, route: &Route, query_token: Option<&str>) -> Result<bool, Reply> {
        let host_matches = incoming.host.as_deref().is_some_and(|host| {
            [format!("127.0.0.1:{}", self.port), format!("localhost:{}", self.port)]
                .iter()
                .any(|allowed| host.eq_ignore_ascii_case(allowed))
        });
        if !host_matches {
            return Err(Reply::refusal(
                403,
                "forbidden",
                "the Host header does not name this server",
            ));
        }

        if let Some(given) = query_token {
            let first_load = matches!(route, Route::Page) && incoming.method == "GET";
            // `swap` claims the first load, so two racing loads cannot both have it.
            if first_load && same_token(given, &self.token) && !self.first_load_used.swap(true, Ordering::SeqCst) {
                return Ok(true);
            }
            return Err(Reply::refusal(
                401,
                "unauthorized",
                &format!("the link's token works for the first page load only; send the token in {TOKEN_HEADER}"),
            ));
        }

        match incoming.token.as_deref() {
            None => Err(Reply::refusal(
                401,
                "unauthorized",
                &format!("the {TOKEN_HEADER} header is missing"),
            )),
            Some(given) if !same_token(given, &self.token) => Err(Reply::refusal(
                401,
                "unauthorized",
                &format!("the {TOKEN_HEADER} header is wrong"),
            )),
            Some(_) => Ok(false),
        }
    }

    /// Refuse a request with the wrong method for its route, or a mutation
    /// whose `Origin` is missing or does not name the `Host`.
    fn permit(incoming: &Incoming, route: &Route) -> Result<(), Reply> {
        let writes = matches!(route, Route::Write(_));
        match (writes, incoming.method.as_str()) {
            // A path with no endpoint answers 404 to any method.
            (false, _) if matches!(route, Route::UnknownApi) => Ok(()),
            (false, "GET") => Ok(()),
            (false, _) => Err(Reply::refusal(405, "method_not_allowed", "this path takes GET only")),
            (true, "POST") => {
                // `authorize` passed, so `Host` is present.
                let served = format!("http://{}", incoming.host.as_deref().unwrap_or_default());
                match incoming.origin.as_deref() {
                    None => Err(Reply::refusal(
                        403,
                        "forbidden",
                        "a change needs an Origin header naming this server",
                    )),
                    Some(origin) if !origin.eq_ignore_ascii_case(&served) => Err(Reply::refusal(
                        403,
                        "forbidden",
                        "the Origin header does not name this server",
                    )),
                    Some(_) => Ok(()),
                }
            }
            (true, _) => Err(Reply::refusal(405, "method_not_allowed", "this path takes POST only")),
        }
    }
}

/// The application's read operations, each answering with the CLI's JSON
/// envelope for the same command.
pub trait Reads {
    /// `arcl list`, with the filter flags as `(name, value)` pairs.
    fn list(&self, query: &[(String, String)]) -> Reply;
    /// `arcl show <id>`.
    fn show(&self, id: &str) -> Reply;
    /// `arcl ready`, with the filter flags as `(name, value)` pairs.
    fn ready(&self, query: &[(String, String)]) -> Reply;
    /// `arcl explain <task-id>`.
    fn explain(&self, task_id: &str) -> Reply;
    /// `arcl context <task-id>`.
    fn context(&self, task_id: &str) -> Reply;
    /// `arcl tree`.
    fn tree(&self) -> Reply;
    /// Every task with its readiness and effective phase, and every phase,
    /// for the Work list.
    fn work(&self) -> Reply;
}

/// The application's mutations, each answering with the CLI's JSON
/// envelope for the same command. Each runs as the CLI runs a write, with the
/// same validation and errors.
pub trait Mutations {
    /// `arcl task create`.
    fn create_task(&self, input: NewTask) -> Reply;
    /// `arcl task update <id>`.
    fn update_task(&self, id: &str, input: TaskEdit) -> Reply;
    /// `arcl task start <id>`.
    fn start_task(&self, id: &str, input: TaskStart) -> Reply;
    /// `arcl task park <id>`.
    fn park_task(&self, id: &str) -> Reply;
    /// `arcl task unpark <id>`.
    fn unpark_task(&self, id: &str) -> Reply;
    /// `arcl task handoff <id>`.
    fn hand_off_task(&self, id: &str, input: Handoff) -> Reply;
    /// `arcl task complete <id>`.
    fn complete_task(&self, id: &str, input: Completion) -> Reply;
    /// `arcl capture create`.
    fn create_capture(&self, input: NewCapture) -> Reply;
    /// `arcl capture promote <id>`.
    fn promote_capture(&self, id: &str, input: CapturePromotion) -> Reply;
    /// `arcl capture discard <id>`.
    fn discard_capture(&self, id: &str) -> Reply;
    /// `arcl spec create`.
    fn create_spec(&self, input: NewSpec) -> Reply;
    /// `arcl plan create`, without structured input.
    fn create_plan(&self, input: NewPlan) -> Reply;
    /// `arcl plan apply <id>`.
    fn apply_plan(&self, id: &str, input: PlanInput) -> Reply;
}

/// Answer an admitted read or page request.
fn dispatch(route: Route, reads: &dyn Reads) -> Answer {
    match route {
        Route::List(query) => Answer::json(reads.list(&query)),
        Route::Show(id) => Answer::json(reads.show(&id)),
        Route::Ready(query) => Answer::json(reads.ready(&query)),
        Route::Explain(id) => Answer::json(reads.explain(&id)),
        Route::Context(id) => Answer::json(reads.context(&id)),
        Route::Tree { has_query: true } => {
            Answer::json(Reply::invalid_request("`/api/tree` takes no query parameters"))
        }
        Route::Tree { has_query: false } => Answer::json(reads.tree()),
        Route::Work { has_query: true } => {
            Answer::json(Reply::invalid_request("`/api/work` takes no query parameters"))
        }
        Route::Work { has_query: false } => Answer::json(reads.work()),
        // `handle` sends mutations to the mutation thread.
        Route::Write(_) => Answer::json(Reply::internal("a mutation reached the read path")),
        Route::UnknownApi => Answer::json(Reply::refusal(404, "not_found", "no endpoint has this path")),
        Route::Page => page(),
    }
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

/// Whether `headers` declare a body over [`MAX_BODY_BYTES`]. hyper refuses a
/// malformed `Content-Length` before the request gets here.
fn declares_too_much(headers: &HeaderMap) -> bool {
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
async fn read_body(body: Body, deadline: Duration) -> Result<Vec<u8>, Answer> {
    match tokio::time::timeout(deadline, Limited::new(body, MAX_BODY_BYTES).collect()).await {
        Err(_) => Err(Answer::json(Reply::timed_out())),
        Ok(Ok(collected)) => Ok(collected.to_bytes().to_vec()),
        Ok(Err(error)) if error.is::<LengthLimitError>() => Err(Answer::json(Reply::too_large())),
        Ok(Err(_)) => Err(Answer::json(Reply::invalid_request(
            "the request body could not be read",
        ))),
    }
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

/// Parse a mutation's body and run it.
fn mutate(write: Write, body: &[u8], mutations: &dyn Mutations) -> Reply {
    match write {
        Write::Create => with_input(body, |input| mutations.create_task(input)),
        Write::Update(id) => with_input(body, |input| mutations.update_task(&id, input)),
        Write::Start(id) => with_input(body, |input| mutations.start_task(&id, input)),
        Write::Park(id) => with_input(body, |NoOptions {}| mutations.park_task(&id)),
        Write::Unpark(id) => with_input(body, |NoOptions {}| mutations.unpark_task(&id)),
        Write::Handoff(id) => with_input(body, |input| mutations.hand_off_task(&id, input)),
        Write::Complete(id) => with_input(body, |input| mutations.complete_task(&id, input)),
        Write::CreateCapture => with_input(body, |input| mutations.create_capture(input)),
        Write::PromoteCapture(id) => with_input(body, |input| mutations.promote_capture(&id, input)),
        Write::DiscardCapture(id) => with_input(body, |NoOptions {}| mutations.discard_capture(&id)),
        Write::CreateSpec => with_input(body, |input| mutations.create_spec(input)),
        Write::CreatePlan => with_input(body, |input| mutations.create_plan(input)),
        Write::ApplyPlan(id) => with_input(body, |input| mutations.apply_plan(&id, input)),
    }
}

/// Parse `body` as the JSON object of a command's options and pass them to
/// `run`. An empty body is an empty object.
fn with_input<T: DeserializeOwned>(body: &[u8], run: impl FnOnce(T) -> Reply) -> Reply {
    let body = if body.iter().all(u8::is_ascii_whitespace) { b"{}".as_slice() } else { body };
    match serde_json::from_slice::<T>(body) {
        Ok(input) => run(input),
        Err(error) => Reply::invalid_request(&format!("the request body is not valid: {error}")),
    }
}

/// Generate a token from the operating system's random source.
fn new_token() -> Result<String, ServeError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(ServeError::Token)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Compare tokens in time that does not depend on where they differ.
fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0_u8, |difference, (left, right)| difference | (left ^ right))
            == 0
}

/// The first value of the token query parameter, if any.
fn token_in(query: &[(String, String)]) -> Option<String> {
    query
        .iter()
        .find(|(name, _)| name == TOKEN_QUERY)
        .map(|(_, value)| value.clone())
}

/// Split a request target into its decoded path and query pairs.
fn split_url(url: &str) -> Result<(String, Vec<(String, String)>), String> {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let path = decode(path, false).ok_or_else(|| "the path is not valid percent-encoded UTF-8".to_owned())?;
    let mut pairs = Vec::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let decoded = decode(name, true).zip(decode(value, true));
        pairs.push(decoded.ok_or_else(|| "the query is not valid percent-encoded UTF-8".to_owned())?);
    }
    Ok((path, pairs))
}

/// Decode `%XX` escapes, and `+` as a space when `plus_is_space`.
fn decode(input: &str, plus_is_space: bool) -> Option<String> {
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = bytes.get(index + 1..index + 3)?;
                output.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
                index += 3;
            }
            b'+' if plus_is_space => {
                output.push(b' ');
                index += 1;
            }
            byte => {
                output.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(output).ok()
}

/// Route a decoded path and its query.
fn route(path: &str, query: Vec<(String, String)>) -> Route {
    let segments = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        ["api", "records"] => Route::List(query),
        ["api", "records", id] if !id.is_empty() => Route::Show((*id).to_owned()),
        ["api", "ready"] => Route::Ready(query),
        ["api", "tasks", id, "explain"] if !id.is_empty() => Route::Explain((*id).to_owned()),
        ["api", "tasks", id, "context"] if !id.is_empty() => Route::Context((*id).to_owned()),
        ["api", "tree"] => Route::Tree { has_query: !query.is_empty() },
        ["api", "work"] => Route::Work { has_query: !query.is_empty() },
        ["api", "tasks"] => Route::Write(Write::Create),
        ["api", "captures"] => Route::Write(Write::CreateCapture),
        ["api", "captures", id, "promote"] if !id.is_empty() => Route::Write(Write::PromoteCapture((*id).to_owned())),
        ["api", "captures", id, "discard"] if !id.is_empty() => Route::Write(Write::DiscardCapture((*id).to_owned())),
        ["api", "specs"] => Route::Write(Write::CreateSpec),
        ["api", "plans"] => Route::Write(Write::CreatePlan),
        ["api", "plans", id, "apply"] if !id.is_empty() => Route::Write(Write::ApplyPlan((*id).to_owned())),
        ["api", "tasks", id, action] if !id.is_empty() => {
            let id = (*id).to_owned();
            match *action {
                "update" => Route::Write(Write::Update(id)),
                "start" => Route::Write(Write::Start(id)),
                "park" => Route::Write(Write::Park(id)),
                "unpark" => Route::Write(Write::Unpark(id)),
                "handoff" => Route::Write(Write::Handoff(id)),
                "complete" => Route::Write(Write::Complete(id)),
                _ => Route::UnknownApi,
            }
        }
        ["api", ..] => Route::UnknownApi,
        _ => Route::Page,
    }
}

/// Serve the web app page.
fn page() -> Answer {
    Answer {
        status: 200,
        content_type: "text/html; charset=utf-8",
        body: INDEX_HTML.unwrap_or(NOT_BUILT_PAGE.as_bytes()).to_vec(),
        allow: None,
        retry_after: None,
        csp: PAGE_CSP.as_str(),
        close: false,
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};
    use std::net::{Shutdown, TcpStream};
    use std::time::Instant;

    use super::*;
    use sha2::Digest;

    const TOKEN: &str = "abc123";
    const PORT: u16 = 4321;

    /// Reads that echo which operation ran and with what.
    struct Echo;

    impl Reads for Echo {
        fn list(&self, query: &[(String, String)]) -> Reply {
            Reply::success(format!("list {query:?}"))
        }
        fn show(&self, id: &str) -> Reply {
            Reply::success(format!("show {id}"))
        }
        fn ready(&self, query: &[(String, String)]) -> Reply {
            Reply::success(format!("ready {query:?}"))
        }
        fn explain(&self, task_id: &str) -> Reply {
            Reply::success(format!("explain {task_id}"))
        }
        fn context(&self, task_id: &str) -> Reply {
            Reply::failure(5, format!("context {task_id}"))
        }
        fn tree(&self) -> Reply {
            Reply::success("tree".to_owned())
        }
        fn work(&self) -> Reply {
            Reply::success("work".to_owned())
        }
    }

    impl Mutations for Echo {
        fn create_task(&self, input: NewTask) -> Reply {
            Reply::success(format!("create {input:?}"))
        }
        fn update_task(&self, id: &str, input: TaskEdit) -> Reply {
            Reply::success(format!("update {id} {input:?}"))
        }
        fn start_task(&self, id: &str, input: TaskStart) -> Reply {
            Reply::success(format!("start {id} {input:?}"))
        }
        fn park_task(&self, id: &str) -> Reply {
            Reply::success(format!("park {id}"))
        }
        fn unpark_task(&self, id: &str) -> Reply {
            Reply::success(format!("unpark {id}"))
        }
        fn hand_off_task(&self, id: &str, input: Handoff) -> Reply {
            Reply::success(format!("handoff {id} {input:?}"))
        }
        fn complete_task(&self, id: &str, input: Completion) -> Reply {
            Reply::failure(4, format!("complete {id} {input:?}"))
        }
        fn create_capture(&self, input: NewCapture) -> Reply {
            Reply::success(format!("capture {input:?}"))
        }
        fn promote_capture(&self, id: &str, input: CapturePromotion) -> Reply {
            Reply::success(format!("promote {id} {input:?}"))
        }
        fn discard_capture(&self, id: &str) -> Reply {
            Reply::success(format!("discard {id}"))
        }
        fn create_spec(&self, input: NewSpec) -> Reply {
            Reply::success(format!("spec {input:?}"))
        }
        fn create_plan(&self, input: NewPlan) -> Reply {
            Reply::success(format!("plan {input:?}"))
        }
        fn apply_plan(&self, id: &str, input: PlanInput) -> Reply {
            Reply::success(format!("apply {id} {input:?}"))
        }
    }

    fn gate() -> Gate {
        Gate::new(TOKEN.to_owned(), PORT)
    }

    /// Check and answer `incoming` as [`Server::run`] does, with an empty body.
    fn answered(gate: &Gate, incoming: &Incoming) -> Answer {
        answered_with(gate, incoming, b"")
    }

    /// Check and answer `incoming` as [`Server::run`] does, with `body` as
    /// the body read.
    fn answered_with(gate: &Gate, incoming: &Incoming, body: &[u8]) -> Answer {
        match gate.admit(incoming) {
            Ok(Route::Write(write)) => Answer::json(mutate(write, body, &Echo)),
            Ok(route) => dispatch(route, &Echo),
            Err(answer) => answer,
        }
    }

    /// Serve on an ephemeral port on a background thread, and return the port
    /// and the token.
    fn serve(body_deadline: Duration) -> (u16, String) {
        serve_with(Limits { body: body_deadline, ..Limits::default() })
    }

    /// Serve as [`serve`] does, with these limits.
    fn serve_with(limits: Limits) -> (u16, String) {
        let mut server = Server::bind(None).expect("a server");
        server.limits = limits;
        let port = server.port();
        let token = server.gate.token.clone();
        thread::spawn(move || server.run_until(&Echo, &Echo, std::future::pending()));
        (port, token)
    }

    /// Open a connection and send `head` and `body`, then leave it open.
    fn send(port: u16, head: &str, body: &str) -> TcpStream {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("a read timeout");
        write!(stream, "{head}\r\nHost: 127.0.0.1:{port}\r\n").expect("a write");
        stream.write_all(body.as_bytes()).expect("a write");
        stream
    }

    /// Read a response to its end, or fail after the read timeout.
    fn response(mut stream: TcpStream) -> String {
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("a response before the read timeout");
        response
    }

    /// Read the status line of a response on a connection that stays open.
    fn status_line(stream: &mut TcpStream) -> String {
        let mut line = Vec::new();
        let mut byte = [0_u8];
        while !line.ends_with(b"\r\n") {
            stream
                .read_exact(&mut byte)
                .expect("a status line before the read timeout");
            line.push(byte[0]);
        }
        String::from_utf8(line).expect("UTF-8")
    }

    /// An authorized `GET /api/records` and its response.
    fn authorized_get(port: u16, token: &str) -> String {
        let head = format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nConnection: close");
        response(send(port, &head, "\r\n"))
    }

    fn get(url: &str, token: Option<&str>) -> Incoming {
        Incoming {
            method: "GET".to_owned(),
            url: url.to_owned(),
            host: Some(format!("127.0.0.1:{PORT}")),
            origin: None,
            token: token.map(str::to_owned),
        }
    }

    /// A POST from the served origin with the token.
    fn post(url: &str) -> Incoming {
        Incoming {
            method: "POST".to_owned(),
            origin: Some(format!("http://127.0.0.1:{PORT}")),
            ..get(url, Some(TOKEN))
        }
    }

    fn text(answer: Answer) -> (u16, String) {
        (answer.status, String::from_utf8(answer.body).expect("UTF-8"))
    }

    fn error_code(answer: &Answer) -> String {
        let body: serde_json::Value = serde_json::from_slice(&answer.body).expect("a JSON body");
        body["error"]["code"].as_str().expect("an error code").to_owned()
    }

    #[test]
    fn serve_refuses_a_missing_token() {
        let answer = answered(&gate(), &get("/api/records", None));
        assert_eq!(answer.status, 401);
        assert_eq!(error_code(&answer), "unauthorized");
    }

    #[test]
    fn serve_refuses_a_wrong_token() {
        let answer = answered(&gate(), &get("/api/records", Some("abc124")));
        assert_eq!(answer.status, 401);
        let answer = answered(&gate(), &get("/", Some("abc")));
        assert_eq!(answer.status, 401);
    }

    #[test]
    fn serve_refuses_a_wrong_host() {
        for host in [
            None,
            Some("evil.example:4321"),
            Some("127.0.0.1:1234"),
            Some("127.0.0.1"),
        ] {
            let incoming = Incoming { host: host.map(str::to_owned), ..get("/api/records", Some(TOKEN)) };
            let answer = answered(&gate(), &incoming);
            assert_eq!(answer.status, 403, "{host:?}");
            assert_eq!(error_code(&answer), "forbidden");
        }
        let incoming = Incoming { host: Some(format!("LOCALHOST:{PORT}")), ..get("/api/records", Some(TOKEN)) };
        assert_eq!(answered(&gate(), &incoming).status, 200);
    }

    #[test]
    fn serve_accepts_the_query_token_for_the_first_page_load_only() {
        let gate = gate();
        let wrong = answered(&gate, &get("/?token=nope", None));
        assert_eq!(wrong.status, 401);

        let first = answered(&gate, &get(&format!("/?token={TOKEN}"), None));
        assert_eq!(first.status, 200);
        assert_eq!(first.content_type, "text/html; charset=utf-8");

        let again = answered(&gate, &get(&format!("/?token={TOKEN}"), None));
        assert_eq!(again.status, 401);
        let with_header = answered(&gate, &get(&format!("/work?token={TOKEN}"), Some(TOKEN)));
        assert_eq!(with_header.status, 401);
        assert_eq!(answered(&gate, &get("/", Some(TOKEN))).status, 200);
    }

    #[test]
    fn serve_never_accepts_a_query_token_on_the_api() {
        let answer = answered(&gate(), &get(&format!("/api/records?token={TOKEN}"), None));
        assert_eq!(answer.status, 401);
        let answer = answered(&gate(), &get(&format!("/api/records/x?token={TOKEN}"), Some(TOKEN)));
        assert_eq!(answer.status, 401);
    }

    #[test]
    fn serve_checks_the_host_and_token_on_the_work_endpoint() {
        assert_eq!(answered(&gate(), &get("/api/work", None)).status, 401);
        assert_eq!(answered(&gate(), &get("/api/work", Some("abc124"))).status, 401);
        let wrong_host = Incoming { host: Some("evil.example:4321".to_owned()), ..get("/api/work", Some(TOKEN)) };
        assert_eq!(answered(&gate(), &wrong_host).status, 403);
        let query_token = answered(&gate(), &get(&format!("/api/work?token={TOKEN}"), None));
        assert_eq!(query_token.status, 401);
        assert_eq!(answered(&gate(), &get("/api/work", Some(TOKEN))).status, 200);
    }

    #[test]
    fn serve_answers_reads_only_on_get() {
        let incoming = post("/api/records");
        let answer = answered(&gate(), &incoming);
        assert_eq!(answer.status, 405);
        assert!(answer.headers().contains(&("Allow", "GET")));
        let anonymous = Incoming { method: "DELETE".to_owned(), ..get("/api/records", None) };
        assert_eq!(answered(&gate(), &anonymous).status, 401);
    }

    #[test]
    fn serve_answers_mutations_only_on_post() {
        for url in [
            "/api/tasks",
            "/api/tasks/arcl-t-1/start",
            "/api/tasks/arcl-t-1/complete",
            "/api/captures",
            "/api/captures/arcl-c-1/promote",
            "/api/captures/arcl-c-1/discard",
            "/api/specs",
            "/api/plans",
            "/api/plans/arcl-pl-1/apply",
        ] {
            for method in ["GET", "PUT", "DELETE", "OPTIONS"] {
                let incoming = Incoming { method: method.to_owned(), ..post(url) };
                let answer = answered(&gate(), &incoming);
                assert_eq!(answer.status, 405, "{method} {url}");
                assert!(answer.headers().contains(&("Allow", "POST")));
            }
        }
    }

    #[test]
    fn serve_mutations_need_the_served_origin() {
        for origin in [
            None,
            Some("http://evil.example"),
            Some("http://127.0.0.1:1234"),
            Some("http://localhost:4321"),
            Some("https://127.0.0.1:4321"),
            Some("null"),
        ] {
            for url in [
                "/api/tasks/arcl-t-1/park",
                "/api/captures",
                "/api/captures/arcl-c-1/discard",
                "/api/specs",
            ] {
                let incoming = Incoming { origin: origin.map(str::to_owned), ..post(url) };
                let answer = answered_with(&gate(), &incoming, br#"{"title":"T"}"#);
                assert_eq!(answer.status, 403, "{origin:?} {url}");
                assert_eq!(error_code(&answer), "forbidden");
            }
        }
        let incoming = Incoming {
            host: Some(format!("localhost:{PORT}")),
            origin: Some(format!("http://localhost:{PORT}")),
            ..post("/api/tasks/arcl-t-1/park")
        };
        assert_eq!(text(answered(&gate(), &incoming)), (200, "park arcl-t-1".to_owned()));
    }

    #[test]
    fn serve_mutations_still_need_the_token_and_host() {
        let missing = Incoming { token: None, ..post("/api/tasks") };
        assert_eq!(answered(&gate(), &missing).status, 401);
        let host = Incoming { host: Some("evil.example".to_owned()), ..post("/api/tasks") };
        assert_eq!(answered(&gate(), &host).status, 403);
    }

    #[test]
    fn serve_routes_the_mutations() {
        let routed = |url: &str, body: &str| text(answered_with(&gate(), &post(url), body.as_bytes()));
        assert_eq!(
            routed("/api/tasks", r#"{"title":"New","priority":"high"}"#),
            (
                200,
                r#"create NewTask { title: "New", body: None, priority: Some("high"), parent: None, spec: None, plan: None, phase: None, blocked_by: [] }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/tasks/arcl-t-1/update", r#"{"body":"Text"}"#),
            (
                200,
                r#"update arcl-t-1 TaskEdit { title: None, body: Some("Text"), priority: None }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/tasks/arcl-t-1/start", ""),
            (200, "start arcl-t-1 TaskStart { force: false }".to_owned())
        );
        assert_eq!(
            routed("/api/tasks/arcl-t-1/park", "{}"),
            (200, "park arcl-t-1".to_owned())
        );
        assert_eq!(
            routed("/api/tasks/arcl-t-1/unpark", ""),
            (200, "unpark arcl-t-1".to_owned())
        );
        assert_eq!(
            routed("/api/tasks/arcl-t-1/handoff", r#"{"note":"Next"}"#),
            (200, r#"handoff arcl-t-1 Handoff { note: Some("Next") }"#.to_owned())
        );
        assert_eq!(
            routed("/api/tasks/arcl-t-1/complete", r#"{"evidence":"Done"}"#),
            (
                409,
                r#"complete arcl-t-1 Completion { evidence: Some("Done"), allow_open_children: false, with_subtasks: false }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/tasks", r#"{"title":"New","blocked_by":["arcl-t-2"]}"#),
            (
                200,
                r#"create NewTask { title: "New", body: None, priority: None, parent: None, spec: None, plan: None, phase: None, blocked_by: ["arcl-t-2"] }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/captures", r#"{"title":"Idea","body":"More"}"#),
            (
                200,
                r#"capture NewCapture { title: "Idea", body: Some("More") }"#.to_owned()
            )
        );
        assert_eq!(routed("/api/tasks/arcl-t-1/delete", "").0, 404);
    }

    #[test]
    fn serve_routes_the_planning_mutations() {
        let routed = |url: &str, body: &str| text(answered_with(&gate(), &post(url), body.as_bytes()));
        assert_eq!(
            routed("/api/captures/arcl-c-1/promote", r#"{"to":"task","priority":"high","phase":"arcl-ph-1"}"#),
            (
                200,
                r#"promote arcl-c-1 CapturePromotion { to: "task", title: None, body: None, acceptance_criteria: None, spec: None, plan: None, phase: Some("arcl-ph-1"), parent: None, priority: Some("high") }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/captures/arcl-c-1/discard", ""),
            (200, "discard arcl-c-1".to_owned())
        );
        assert_eq!(
            routed("/api/specs", r#"{"title":"Spec","acceptance_criteria":"- works"}"#),
            (
                200,
                r#"spec NewSpec { title: "Spec", body: None, acceptance_criteria: Some("- works") }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/plans", r#"{"title":"Plan","spec":"arcl-s-1"}"#),
            (
                200,
                r#"plan NewPlan { title: "Plan", spec: "arcl-s-1", body: None }"#.to_owned()
            )
        );
        assert_eq!(
            routed("/api/plans/arcl-pl-1/apply", r#"{"document":"format-version = 1"}"#),
            (
                200,
                r#"apply arcl-pl-1 PlanInput { document: "format-version = 1" }"#.to_owned()
            )
        );
        assert_eq!(routed("/api/captures/arcl-c-1/update", "").0, 404);
        assert_eq!(routed("/api/plans/arcl-pl-1/check", "").0, 404);
        assert_eq!(routed("/api/specs/arcl-s-1", "").0, 404);
    }

    #[test]
    fn serve_refuses_malformed_mutation_bodies() {
        let with_body = |url: &str, body: &[u8]| answered_with(&gate(), &post(url), body);
        for (url, body) in [
            ("/api/tasks", b"{}".as_slice()),
            ("/api/tasks", b"not json"),
            ("/api/tasks/arcl-t-1/park", br#"{"force":true}"#),
            ("/api/tasks/arcl-t-1/update", br#"{"spec_file":"/etc/passwd"}"#),
            ("/api/tasks/arcl-t-1/start", br#"{"force":"yes"}"#),
            ("/api/captures", b"{}"),
            ("/api/captures", br#"{"title":"Idea","priority":"high"}"#),
            ("/api/captures/arcl-c-1/promote", b"{}"),
            ("/api/captures/arcl-c-1/promote", br#"{"to":"task","position":3}"#),
            ("/api/captures/arcl-c-1/discard", br#"{"force":true}"#),
            (
                "/api/specs",
                br#"{"title":"S","acceptance_criteria_file":"/etc/passwd"}"#,
            ),
            ("/api/plans", br#"{"title":"P"}"#),
            (
                "/api/plans",
                br#"{"title":"P","spec":"arcl-s-1","input":"/etc/passwd"}"#,
            ),
            ("/api/plans/arcl-pl-1/apply", br#"{"file":"/etc/passwd"}"#),
            ("/api/plans/arcl-pl-1/apply", b""),
        ] {
            let refused = with_body(url, body);
            assert_eq!(refused.status, 400, "{url} {body:?}");
            assert_eq!(error_code(&refused), "invalid_request");
        }
    }

    #[test]
    fn serve_sends_no_cors_headers() {
        let answer = answered(&gate(), &get("/api/records", Some(TOKEN)));
        assert!(
            answer
                .headers()
                .iter()
                .all(|(name, _)| !name.to_ascii_lowercase().starts_with("access-control-"))
        );
    }

    #[test]
    fn serve_routes_the_read_endpoints() {
        let body = |url: &str| {
            let answer = answered(&gate(), &get(url, Some(TOKEN)));
            (answer.status, String::from_utf8(answer.body).expect("UTF-8"))
        };
        assert_eq!(
            body("/api/records?kind=task&status=pending&status=in%5Fprogress"),
            (
                200,
                r#"list [("kind", "task"), ("status", "pending"), ("status", "in_progress")]"#.to_owned()
            )
        );
        assert_eq!(body("/api/records/arcl-t-1"), (200, "show arcl-t-1".to_owned()));
        assert_eq!(
            body("/api/ready?priority=high"),
            (200, r#"ready [("priority", "high")]"#.to_owned())
        );
        assert_eq!(
            body("/api/tasks/arcl-t-1/explain"),
            (200, "explain arcl-t-1".to_owned())
        );
        assert_eq!(
            body("/api/tasks/arcl-t-1/context"),
            (404, "context arcl-t-1".to_owned())
        );
        assert_eq!(body("/api/work"), (200, "work".to_owned()));
        assert_eq!(body("/api/tree"), (200, "tree".to_owned()));
        assert_eq!(body("/api/tree?id=arcl-s-1").0, 400);
        let (status, refused) = body("/api/work?status=pending");
        assert_eq!(status, 400);
        assert!(refused.contains("invalid_request"), "{refused}");
        assert_eq!(body("/api/work/x").0, 404);
        assert_eq!(body("/api/nothing").0, 404);
        assert_eq!(body("/api/records/a%ZZ").0, 400);
    }

    #[test]
    fn serve_falls_back_to_the_app_page() {
        let answer = answered(&gate(), &get("/planning/anything", Some(TOKEN)));
        assert_eq!(answer.status, 200);
        assert_eq!(answer.content_type, "text/html; charset=utf-8");
    }

    #[test]
    fn serve_sends_a_content_security_policy() {
        let csp = |answer: &Answer| {
            let headers = answer.headers();
            let found = headers.iter().find(|(name, _)| *name == "Content-Security-Policy");
            found.expect("a Content-Security-Policy header").1.to_owned()
        };
        let page = answered(&gate(), &get("/", Some(TOKEN)));
        let policy = csp(&page);
        for directive in [
            "default-src 'self'",
            "style-src 'self' 'unsafe-inline'",
            "img-src 'self' data:",
            "font-src 'self' data:",
            "connect-src 'self'",
            "object-src 'none'",
            "frame-ancestors 'none'",
            "base-uri 'none'",
            "form-action 'self'",
        ] {
            assert!(
                policy.split("; ").any(|part| part == directive),
                "{directive} in {policy}"
            );
        }
        let script_src = policy
            .split("; ")
            .find(|part| part.starts_with("script-src "))
            .expect("script-src");
        assert!(!script_src.contains("unsafe"), "{script_src}");
        // Each inline script in the page runs, and nothing else inline does.
        let html = String::from_utf8(page.body).expect("UTF-8");
        let scripts: Vec<String> = html
            .split("<script>")
            .skip(1)
            .map(|rest| rest.split("</script>").next().expect("a closing tag"))
            .map(|script| format!("'sha256-{}'", base64(&sha2::Sha256::digest(script.as_bytes()))))
            .collect();
        let sources: Vec<&str> = script_src.split(' ').skip(2).collect();
        assert_eq!(sources, scripts.iter().map(String::as_str).collect::<Vec<_>>());

        let api = answered(&gate(), &get("/api/records", Some(TOKEN)));
        assert_eq!(csp(&api), API_CSP);
        assert!(API_CSP.starts_with("default-src 'none'"));
    }

    /// `bytes` in padded standard base64, as CSP hash sources write them.
    fn base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let word = chunk
                .iter()
                .enumerate()
                .fold(0u32, |word, (i, byte)| word | u32::from(*byte) << (16 - 8 * i));
            for i in 0..4 {
                out.push(if i <= chunk.len() {
                    char::from(ALPHABET[(word >> (18 - 6 * i)) as usize & 63])
                } else {
                    '='
                });
            }
        }
        out
    }

    #[test]
    fn serve_maps_cli_exit_codes_to_statuses() {
        assert_eq!(Reply::failure(2, String::new()).status(), 400);
        assert_eq!(Reply::failure(3, String::new()).status(), 400);
        assert_eq!(Reply::failure(4, String::new()).status(), 409);
        assert_eq!(Reply::failure(5, String::new()).status(), 404);
        assert_eq!(Reply::failure(1, String::new()).status(), 500);
    }

    #[test]
    fn serve_tokens_are_random_hex() {
        let first = new_token().expect("a token");
        assert_eq!(first.len(), TOKEN_BYTES * 2);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_ne!(first, new_token().expect("a token"));
    }

    #[test]
    fn serve_refuses_a_stalled_body_without_waiting_for_it() {
        let (port, token) = serve(BODY_DEADLINE);
        // No token, a large declared body, ten bytes of it, then silence.
        let mut stalled = send(
            port,
            "POST /api/tasks HTTP/1.1\r\nContent-Length: 100000",
            "\r\n0123456789",
        );
        thread::sleep(Duration::from_millis(200));
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert_eq!(status_line(&mut stalled), "HTTP/1.1 401 Unauthorized\r\n");
    }

    #[test]
    fn serve_answers_busy_with_retry_after() {
        let answer = Answer::json(Reply::busy(r#"{"error":{}}"#.to_owned()));
        assert_eq!(answer.status, 503);
        assert!(answer.headers().contains(&("Retry-After", RETRY_AFTER_SECONDS)));
        let plain = Answer::json(Reply::failure(1, String::new()));
        assert!(plain.headers().iter().all(|(name, _)| *name != "Retry-After"));
    }

    #[test]
    fn serve_refuses_a_huge_declared_body_and_keeps_serving() {
        let (port, token) = serve(BODY_DEADLINE);
        let huge = "Content-Length: 1000000000000000";
        let origin = format!("Origin: http://127.0.0.1:{port}");
        for head in [
            format!("POST /api/tasks HTTP/1.1\r\n{huge}"),
            format!("POST /api/tasks HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\n{origin}\r\n{huge}"),
            format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\n{huge}"),
        ] {
            let refused = response(send(port, &head, "\r\n{\"title\":"));
            assert!(refused.starts_with("HTTP/1.1 413"), "{refused}");
            assert!(refused.contains("payload_too_large"), "{refused}");
            assert!(refused.contains(API_CSP), "{refused}");
            assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        }
    }

    #[test]
    fn serve_refuses_a_chunked_body_over_the_limit() {
        let (port, token) = serve(BODY_DEADLINE);
        let head = format!(
            "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nTransfer-Encoding: chunked"
        );
        let over = MAX_BODY_BYTES + 1;
        let body = format!("\r\n{over:x}\r\n{}", " ".repeat(over));
        let mut stream = send(port, &head, &body);
        assert!(status_line(&mut stream).starts_with("HTTP/1.1 413"));
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn serve_times_out_a_stalled_body_while_reads_stay_fast() {
        let (port, token) = serve(Duration::from_millis(300));
        let head = format!(
            "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: 100000"
        );
        let started = Instant::now();
        let stalled = send(port, &head, "\r\n{        ");
        thread::sleep(Duration::from_millis(50));
        // The body read waits on its own task; reads go on meanwhile.
        for _ in 0..3 {
            let read = Instant::now();
            assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
            assert!(read.elapsed() < Duration::from_millis(150), "{:?}", read.elapsed());
        }
        // The 408 arrives at the deadline, with the client still connected.
        let timed_out = response(stalled);
        assert!(timed_out.starts_with("HTTP/1.1 408"), "{timed_out}");
        assert!(timed_out.contains("request_timeout"));
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    }

    #[test]
    fn serve_answers_past_many_stalled_connections() {
        let (port, token) = serve(BODY_DEADLINE);
        let admitted = format!(
            "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: 100000"
        );
        let refused = "POST /api/tasks HTTP/1.1\r\nContent-Length: 100000";
        let held: Vec<TcpStream> = (0..96)
            .map(|index| match index % 4 {
                // Connected, and nothing sent.
                0 => TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection"),
                // Half a request head.
                1 => {
                    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
                    stream
                        .write_all(b"GET /api/records HTTP/1.1\r\nHost: ")
                        .expect("a write");
                    stream
                }
                // An admitted mutation whose body stalls.
                2 => send(port, &admitted, "\r\n{"),
                // A refused request whose body stalls.
                _ => send(port, refused, "\r\n{"),
            })
            .collect();
        thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "{:?}",
            started.elapsed()
        );
        drop(held);
    }

    /// Open a connection, write `request` pipelined `copies` times without
    /// waiting, and never read the replies.
    fn pipeline(port: u16, request: &str, copies: usize) -> TcpStream {
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        stream.set_nonblocking(true).expect("a non-blocking socket");
        // A full send buffer or a closed connection ends the write early,
        // which is all the attack needs.
        let _ = stream.write_all(request.repeat(copies).as_bytes());
        stream
    }

    #[test]
    fn serve_answers_past_clients_that_pipeline_refused_requests_and_never_read() {
        let (port, token) = serve(BODY_DEADLINE);
        let held: Vec<TcpStream> = (0..MAX_CONNECTIONS + 2)
            .map(|_| pipeline(port, "GET / HTTP/1.1\r\nHost: evil\r\n\r\n", 2000))
            .collect();
        thread::sleep(Duration::from_millis(300));
        let started = Instant::now();
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
        drop(held);
    }

    #[test]
    fn serve_closes_a_connection_after_a_refusal_before_the_token() {
        let (port, token) = serve(BODY_DEADLINE);
        let huge = "Content-Length: 1000000000000000";
        for head in [
            "GET /api/records HTTP/1.1\r\nHost: evil".to_owned(),
            "GET /api/records HTTP/1.1".to_owned(),
            format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: wrong"),
            format!("GET /api/records?token={token} HTTP/1.1"),
            "GET /a%ZZ HTTP/1.1".to_owned(),
            format!("POST /api/tasks HTTP/1.1\r\n{huge}"),
        ] {
            // A second request follows on the same connection, unanswered.
            let mut stream = send(port, &head, "\r\n");
            let next =
                format!("GET /api/records HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\r\n");
            let _ = stream.write_all(next.as_bytes());
            let answered = response(stream);
            assert!(
                answered.contains("Connection: close") || answered.contains("connection: close"),
                "{answered}"
            );
            assert_eq!(answered.matches("HTTP/1.1 ").count(), 1, "{answered}");
        }
        // A refusal after the token keeps the connection open.
        let head = format!("POST /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: 0");
        let mut stream = send(port, &head, "\r\n");
        assert_eq!(status_line(&mut stream), "HTTP/1.1 405 Method Not Allowed\r\n");
        let next = format!(
            "GET /api/records HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\nConnection: close\r\n\r\n"
        );
        stream.write_all(next.as_bytes()).expect("a write");
        assert!(response(stream).contains("HTTP/1.1 200 OK"));
    }

    #[test]
    fn serve_closes_an_admitted_connection_that_never_reads() {
        let limits = Limits { connections: 1, write: Duration::from_millis(300), ..Limits::default() };
        let (port, token) = serve_with(limits);
        let request = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\r\n");
        let held = pipeline(port, &request, 2000);
        thread::sleep(Duration::from_millis(100));
        let started = Instant::now();
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
        drop(held);
    }

    #[test]
    fn serve_closes_a_connection_that_stalls_its_head() {
        let (port, token) = serve_with(Limits { header: Duration::from_millis(300), ..Limits::default() });
        let mut idle = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        let mut partial = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        partial
            .write_all(b"GET /api/records HTTP/1.1\r\nHost: ")
            .expect("a write");
        let started = Instant::now();
        for stream in [&mut idle, &mut partial] {
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .expect("a read timeout");
            let mut rest = Vec::new();
            stream.read_to_end(&mut rest).expect("the server closes the connection");
        }
        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(2),
            "{elapsed:?}"
        );
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    }

    #[test]
    fn serve_caps_open_connections() {
        let (port, token) = serve_with(Limits { connections: 4, ..Limits::default() });
        let mut held: Vec<TcpStream> = (0..4)
            .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection"))
            .collect();
        thread::sleep(Duration::from_millis(100));
        // Past the cap, a connection waits to be accepted.
        let head = format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nConnection: close");
        let mut queued = send(port, &head, "\r\n");
        queued
            .set_read_timeout(Some(Duration::from_millis(300)))
            .expect("a read timeout");
        let mut byte = [0_u8];
        let early = queued.read(&mut byte);
        assert!(early.is_err(), "answered past the cap: {early:?}");
        // A client that closes frees its place for the queued connection.
        held.pop()
            .expect("a held stream")
            .shutdown(Shutdown::Both)
            .expect("a shutdown");
        queued
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("a read timeout");
        assert_eq!(status_line(&mut queued), "HTTP/1.1 200 OK\r\n");
    }

    #[test]
    fn serve_stops_when_shutdown_completes() {
        let server = Server::bind(None).expect("a server");
        let (port, token) = (server.port(), server.gate.token.clone());
        let (stop, stopped) = oneshot::channel::<()>();
        let (finished, done) = mpsc::channel();
        thread::spawn(move || {
            let result = server.run_until(&Echo, &Echo, async {
                let _ = stopped.await;
            });
            let _ = finished.send(result.is_ok());
        });
        let _idle = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        stop.send(()).expect("a send");
        // The idle connection closes at once, so the grace period is not spent.
        let returned = done.recv_timeout(Duration::from_secs(2)).expect("run returns");
        assert!(returned);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
    }

    #[test]
    fn serve_reads_an_admitted_body_that_arrives() {
        let (port, token) = serve(BODY_DEADLINE);
        let body = format!("{{\"note\":\"{}\"}}", "n".repeat(4000));
        let head = format!(
            "POST /api/tasks/arcl-t-1/handoff HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close",
            body.len()
        );
        let answered = response(send(port, &head, &format!("\r\n{body}")));
        assert!(answered.starts_with("HTTP/1.1 200"), "{answered}");
        assert!(answered.contains("handoff arcl-t-1"));
    }
}
