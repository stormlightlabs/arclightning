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
//! The server checks a request's head before it reads any of the body, and
//! reads a body only for a mutation that passed every check. Mutations run one
//! at a time on their own thread, so a mutation that waits, for its body or
//! for a lock, never holds up reads and page loads. A body that does not
//! arrive within [`BODY_DEADLINE`] is abandoned. At most [`MAX_HELPERS`]
//! threads wait on the bodies of refused requests, and as many on the bodies
//! of admitted ones; past that, requests queue for a free thread.
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

use std::collections::VecDeque;
use std::io::{self, Cursor, Read};
use std::net::{Ipv4Addr, TcpListener};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, mpsc};
use std::thread;
use std::time::Duration;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tiny_http::{Header, Response};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The request header that carries the per-run token.
pub const TOKEN_HEADER: &str = "X-Arcl-Token";

/// The query parameter that carries the token on the first page load.
pub const TOKEN_QUERY: &str = "token";

/// How long the server waits for a mutation's body. A client that is still
/// sending when it passes gets a 408 once its connection reads again or closes.
pub const BODY_DEADLINE: Duration = Duration::from_secs(5);

/// The most threads that wait on unread request bodies, for each of refused
/// requests and admitted mutations. A thread lives until its client sends the
/// rest of the body or closes, since tiny_http offers no read timeout.
pub const MAX_HELPERS: usize = 16;

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
    Start(Box<dyn std::error::Error + Send + Sync>),
    /// The listening socket stopped accepting requests.
    #[error("could not accept a request: {0}")]
    Accept(io::Error),
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
    http: tiny_http::Server,
    gate: Gate,
    /// How long to wait for a mutation's body; [`BODY_DEADLINE`] outside tests.
    body_deadline: Duration,
}

impl Server {
    /// Bind `127.0.0.1` on `port`, or on an ephemeral port when `port` is
    /// `None`, and generate the token for this run.
    pub fn bind(port: Option<u16>) -> Result<Self, ServeError> {
        let requested = port.unwrap_or(0);
        let bind_error = |source| ServeError::Bind { port: requested, source };
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, requested)).map_err(bind_error)?;
        let port = listener.local_addr().map_err(bind_error)?.port();
        let token = new_token()?;
        let http = tiny_http::Server::from_listener(listener, None).map_err(ServeError::Start)?;
        Ok(Self { http, gate: Gate::new(token, port), body_deadline: BODY_DEADLINE })
    }

    /// The port the server listens on.
    pub const fn port(&self) -> u16 {
        self.gate.port
    }

    /// The URL to open, with the token for the first page load.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?{TOKEN_QUERY}={}", self.gate.port, self.gate.token)
    }

    /// Serve requests until the socket fails.
    ///
    /// A request's head is checked before its body is touched. Reads and
    /// pages are answered on this thread. Admitted mutations go, in order, to
    /// one mutation thread, which reads the body on a helper thread, waits
    /// for it no longer than the body deadline, and runs the mutation.
    pub fn run(self, reads: &dyn Reads, mutations: &(dyn Mutations + Sync)) -> Result<(), ServeError> {
        let Self { http, mut gate, body_deadline } = self;
        let refused = Helpers::new(MAX_HELPERS);
        let bodies = Helpers::new(MAX_HELPERS);
        thread::scope(|scope| {
            let (writes, queued) = mpsc::channel::<(tiny_http::Request, Write)>();
            let refused_for_writes = Arc::clone(&refused);
            scope.spawn(move || {
                for (request, write) in queued {
                    // When the read is abandoned, a helper owns the request
                    // and answers 408.
                    if let Some(BodyRead { request, body, complete }) = read_body(request, body_deadline, &bodies) {
                        let answer = Answer::json(mutate(write, body.as_deref(), mutations));
                        respond(request, &answer, complete, &refused_for_writes);
                    }
                }
            });
            loop {
                let request = http.recv().map_err(ServeError::Accept)?;
                let incoming = Incoming::head(&request);
                match gate.admit(&incoming) {
                    Err(answer) => respond(request, &answer, false, &refused),
                    Ok(Route::Write(write)) => {
                        // The mutation thread stops only by panicking; its
                        // requests drop and answer 500.
                        let _ = writes.send((request, write));
                    }
                    Ok(route) => respond(request, &dispatch(route, reads), false, &refused),
                }
            }
        })
    }
}

/// Threads that finish requests whose body may be unread. At most `cap` run
/// at once; past that, jobs wait in a queue for a thread to finish its job.
struct Helpers {
    cap: usize,
    state: Mutex<HelperState>,
}

impl Helpers {
    fn new(cap: usize) -> Arc<Self> {
        Arc::new(Self { cap, state: Mutex::new(HelperState { running: 0, queue: VecDeque::new() }) })
    }

    /// Run `job` on a helper thread, or queue it while `cap` threads run.
    fn run(self: &Arc<Self>, job: Job) {
        let mut state = self.lock();
        if state.running >= self.cap {
            state.queue.push_back(job);
            return;
        }
        state.running += 1;
        drop(state);
        let helpers = Arc::clone(self);
        thread::spawn(move || {
            let mut job = job;
            loop {
                job();
                let mut state = helpers.lock();
                match state.queue.pop_front() {
                    Some(next) => job = next,
                    None => {
                        state.running -= 1;
                        return;
                    }
                }
            }
        });
    }

    /// The number of helper threads running and of jobs waiting.
    #[cfg(test)]
    fn load(&self) -> (usize, usize) {
        let state = self.lock();
        (state.running, state.queue.len())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HelperState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The helper threads running and the jobs waiting for one.
struct HelperState {
    running: usize,
    queue: VecDeque<Job>,
}

/// Work for a helper thread.
type Job = Box<dyn FnOnce() + Send>;

/// The body of a mutation that takes no options: an empty object.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoOptions {}

/// A mutation's body, read before the deadline.
struct BodyRead {
    request: tiny_http::Request,
    /// The body, or `None` when the read failed.
    body: Option<Vec<u8>>,
    /// Whether the read reached the end of the body.
    complete: bool,
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
    /// The head of `request`, with no body read.
    fn head(request: &tiny_http::Request) -> Self {
        let header = |name: &'static str| {
            request
                .headers()
                .iter()
                .find(|header| header.field.equiv(name))
                .map(|header| header.value.as_str().to_owned())
        };
        Self {
            method: request.method().as_str().to_owned(),
            url: request.url().to_owned(),
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
        }
    }

    /// The HTTP response for this answer.
    fn response(&self) -> Response<Cursor<Vec<u8>>> {
        let mut response = Response::from_data(self.body.clone()).with_status_code(self.status);
        for (name, value) in self.headers() {
            if let Ok(header) = Header::from_bytes(name, value) {
                response = response.with_header(header);
            }
        }
        response
    }
}

/// The checks every request passes before it is routed.
struct Gate {
    token: String,
    port: u16,
    /// Whether a page load has used the query token.
    first_load_used: bool,
}

impl Gate {
    fn new(token: String, port: u16) -> Self {
        Self { token, port, first_load_used: false }
    }

    /// Check the head of `incoming` and route it, or answer with the refusal.
    /// The body plays no part, so a refused request is never read.
    fn admit(&mut self, incoming: &Incoming) -> Result<Route, Answer> {
        let (path, query) =
            split_url(&incoming.url).map_err(|message| Answer::json(Reply::invalid_request(&message)))?;
        let query_token = token_in(&query);
        let route = route(&path, query);
        if let Err(reply) = self.check(incoming, &route, query_token.as_deref()) {
            let allow = (reply.status == 405).then_some(if matches!(route, Route::Write(_)) { "POST" } else { "GET" });
            return Err(Answer { allow, ..Answer::json(reply) });
        }
        Ok(route)
    }

    /// Refuse a request with the wrong `Host`, without the token, with the
    /// wrong method for its route, or a mutation whose `Origin` is missing or
    /// does not name the `Host`.
    fn check(&mut self, incoming: &Incoming, route: &Route, query_token: Option<&str>) -> Result<(), Reply> {
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
            let first_load = matches!(route, Route::Page) && incoming.method == "GET" && !self.first_load_used;
            if first_load && same_token(given, &self.token) {
                self.first_load_used = true;
                return Ok(());
            }
            return Err(Reply::refusal(
                401,
                "unauthorized",
                &format!("the link's token works for the first page load only; send the token in {TOKEN_HEADER}"),
            ));
        }

        match incoming.token.as_deref() {
            None => {
                return Err(Reply::refusal(
                    401,
                    "unauthorized",
                    &format!("the {TOKEN_HEADER} header is missing"),
                ));
            }
            Some(given) if !same_token(given, &self.token) => {
                return Err(Reply::refusal(
                    401,
                    "unauthorized",
                    &format!("the {TOKEN_HEADER} header is wrong"),
                ));
            }
            Some(_) => {}
        }

        let writes = matches!(route, Route::Write(_));
        match (writes, incoming.method.as_str()) {
            // A path with no endpoint answers 404 to any method.
            (false, _) if matches!(route, Route::UnknownApi) => Ok(()),
            (false, "GET") => Ok(()),
            (false, _) => Err(Reply::refusal(405, "method_not_allowed", "this path takes GET only")),
            (true, "POST") => {
                // `host_matches` passed, so `Host` is present.
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
        // `Server::run` sends mutations to the mutation thread.
        Route::Write(_) => Answer::json(Reply::refusal(
            500,
            "internal_error",
            "a mutation reached the read path",
        )),
        Route::UnknownApi => Answer::json(Reply::refusal(404, "not_found", "no endpoint has this path")),
        Route::Page => page(),
    }
}

/// Read the body of `request` on one of `helpers`, waiting up to `deadline`,
/// or return `None` when the deadline passes first.
///
/// When the deadline passes, the helper keeps the request and answers 408
/// once its read ends, which happens when the client sends the rest or closes.
fn read_body(mut request: tiny_http::Request, deadline: Duration, helpers: &Arc<Helpers>) -> Option<BodyRead> {
    // A rendezvous channel: a send succeeds only while the server still waits.
    let (sender, receiver) = mpsc::sync_channel(0);
    helpers.run(Box::new(move || {
        // One byte past the limit tells an oversized body from a full one.
        let limit = u64::try_from(MAX_BODY_BYTES + 1).unwrap_or(u64::MAX);
        let mut body = Vec::new();
        let read = request.as_reader().take(limit).read_to_end(&mut body);
        let complete = read.is_ok() && body.len() <= MAX_BODY_BYTES;
        let done = BodyRead { request, body: read.is_ok().then_some(body), complete };
        if let Err(mpsc::SendError(BodyRead { request, .. })) = sender.send(done) {
            // This is a helper thread already, so the answer may block here.
            let _ = request.respond(Answer::json(Reply::timed_out()).response());
        }
    }));
    receiver.recv_timeout(deadline).ok()
}

/// Write `answer` to `request`. tiny_http drains an unread body when the
/// request drops, which blocks on a client that stops sending, so a request
/// whose body may be unread is answered and dropped on one of `helpers`.
fn respond(request: tiny_http::Request, answer: &Answer, body_read: bool, helpers: &Arc<Helpers>) {
    let response = answer.response();
    // A client that went away is not the server's failure.
    if body_read || request.body_length().is_none_or(|length| length == 0) {
        let _ = request.respond(response);
    } else {
        helpers.run(Box::new(move || {
            let _ = request.respond(response);
        }));
    }
}

/// Parse a mutation's body and run it.
fn mutate(write: Write, body: Option<&[u8]>, mutations: &dyn Mutations) -> Reply {
    let Some(body) = body else {
        return Reply::invalid_request("the request body could not be read");
    };
    if body.len() > MAX_BODY_BYTES {
        return Reply::refusal(413, "payload_too_large", "the request body is over 1 MiB");
    }
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
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::net::{Shutdown, TcpStream};

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
    fn answered(gate: &mut Gate, incoming: &Incoming) -> Answer {
        answered_with(gate, incoming, Some(b""))
    }

    /// Check and answer `incoming` as [`Server::run`] does, with `body` as
    /// the body read, or `None` for a failed read.
    fn answered_with(gate: &mut Gate, incoming: &Incoming, body: Option<&[u8]>) -> Answer {
        match gate.admit(incoming) {
            Ok(Route::Write(write)) => Answer::json(mutate(write, body, &Echo)),
            Ok(route) => dispatch(route, &Echo),
            Err(answer) => answer,
        }
    }

    /// Serve on an ephemeral port on a background thread, and return the port
    /// and the token.
    fn serve(body_deadline: Duration) -> (u16, String) {
        let mut server = Server::bind(None).expect("a server");
        server.body_deadline = body_deadline;
        let port = server.port();
        let token = server.gate.token.clone();
        thread::spawn(move || server.run(&Echo, &Echo));
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
        let answer = answered(&mut gate(), &get("/api/records", None));
        assert_eq!(answer.status, 401);
        assert_eq!(error_code(&answer), "unauthorized");
    }

    #[test]
    fn serve_refuses_a_wrong_token() {
        let answer = answered(&mut gate(), &get("/api/records", Some("abc124")));
        assert_eq!(answer.status, 401);
        let answer = answered(&mut gate(), &get("/", Some("abc")));
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
            let answer = answered(&mut gate(), &incoming);
            assert_eq!(answer.status, 403, "{host:?}");
            assert_eq!(error_code(&answer), "forbidden");
        }
        let incoming = Incoming { host: Some(format!("LOCALHOST:{PORT}")), ..get("/api/records", Some(TOKEN)) };
        assert_eq!(answered(&mut gate(), &incoming).status, 200);
    }

    #[test]
    fn serve_accepts_the_query_token_for_the_first_page_load_only() {
        let mut gate = gate();
        let wrong = answered(&mut gate, &get("/?token=nope", None));
        assert_eq!(wrong.status, 401);

        let first = answered(&mut gate, &get(&format!("/?token={TOKEN}"), None));
        assert_eq!(first.status, 200);
        assert_eq!(first.content_type, "text/html; charset=utf-8");

        let again = answered(&mut gate, &get(&format!("/?token={TOKEN}"), None));
        assert_eq!(again.status, 401);
        let with_header = answered(&mut gate, &get(&format!("/work?token={TOKEN}"), Some(TOKEN)));
        assert_eq!(with_header.status, 401);
        assert_eq!(answered(&mut gate, &get("/", Some(TOKEN))).status, 200);
    }

    #[test]
    fn serve_never_accepts_a_query_token_on_the_api() {
        let answer = answered(&mut gate(), &get(&format!("/api/records?token={TOKEN}"), None));
        assert_eq!(answer.status, 401);
        let answer = answered(&mut gate(), &get(&format!("/api/records/x?token={TOKEN}"), Some(TOKEN)));
        assert_eq!(answer.status, 401);
    }

    #[test]
    fn serve_checks_the_host_and_token_on_the_work_endpoint() {
        assert_eq!(answered(&mut gate(), &get("/api/work", None)).status, 401);
        assert_eq!(answered(&mut gate(), &get("/api/work", Some("abc124"))).status, 401);
        let wrong_host = Incoming { host: Some("evil.example:4321".to_owned()), ..get("/api/work", Some(TOKEN)) };
        assert_eq!(answered(&mut gate(), &wrong_host).status, 403);
        let query_token = answered(&mut gate(), &get(&format!("/api/work?token={TOKEN}"), None));
        assert_eq!(query_token.status, 401);
        assert_eq!(answered(&mut gate(), &get("/api/work", Some(TOKEN))).status, 200);
    }

    #[test]
    fn serve_answers_reads_only_on_get() {
        let incoming = post("/api/records");
        let answer = answered(&mut gate(), &incoming);
        assert_eq!(answer.status, 405);
        assert!(answer.headers().contains(&("Allow", "GET")));
        let anonymous = Incoming { method: "DELETE".to_owned(), ..get("/api/records", None) };
        assert_eq!(answered(&mut gate(), &anonymous).status, 401);
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
                let answer = answered(&mut gate(), &incoming);
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
                let answer = answered_with(&mut gate(), &incoming, Some(br#"{"title":"T"}"#));
                assert_eq!(answer.status, 403, "{origin:?} {url}");
                assert_eq!(error_code(&answer), "forbidden");
            }
        }
        let incoming = Incoming {
            host: Some(format!("localhost:{PORT}")),
            origin: Some(format!("http://localhost:{PORT}")),
            ..post("/api/tasks/arcl-t-1/park")
        };
        assert_eq!(
            text(answered(&mut gate(), &incoming)),
            (200, "park arcl-t-1".to_owned())
        );
    }

    #[test]
    fn serve_mutations_still_need_the_token_and_host() {
        let missing = Incoming { token: None, ..post("/api/tasks") };
        assert_eq!(answered(&mut gate(), &missing).status, 401);
        let host = Incoming { host: Some("evil.example".to_owned()), ..post("/api/tasks") };
        assert_eq!(answered(&mut gate(), &host).status, 403);
    }

    #[test]
    fn serve_routes_the_mutations() {
        let routed = |url: &str, body: &str| text(answered_with(&mut gate(), &post(url), Some(body.as_bytes())));
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
        let routed = |url: &str, body: &str| text(answered_with(&mut gate(), &post(url), Some(body.as_bytes())));
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
        let with_body = |url: &str, body: &[u8]| answered_with(&mut gate(), &post(url), Some(body));
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
        let large = vec![b' '; MAX_BODY_BYTES + 1];
        assert_eq!(with_body("/api/tasks/arcl-t-1/park", &large).status, 413);
        assert_eq!(
            answered_with(&mut gate(), &post("/api/tasks/arcl-t-1/park"), None).status,
            400
        );
    }

    #[test]
    fn serve_sends_no_cors_headers() {
        let answer = answered(&mut gate(), &get("/api/records", Some(TOKEN)));
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
            let answer = answered(&mut gate(), &get(url, Some(TOKEN)));
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
        let answer = answered(&mut gate(), &get("/planning/anything", Some(TOKEN)));
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
        let page = answered(&mut gate(), &get("/", Some(TOKEN)));
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

        let api = answered(&mut gate(), &get("/api/records", Some(TOKEN)));
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
    fn serve_caps_the_threads_that_wait_on_refused_bodies() {
        let (port, token) = serve(BODY_DEADLINE);
        let stall = || {
            // tiny_http hands a connection that arrives while its idle
            // threads are still waking to a busy thread, so space them out.
            thread::sleep(Duration::from_millis(30));
            send(
                port,
                "POST /api/tasks HTTP/1.1\r\nContent-Length: 100000",
                "\r\n0123456789",
            )
        };
        // Each helper answers, then waits on the rest of its client's body.
        let mut held: Vec<TcpStream> = (0..MAX_HELPERS).map(|_| stall()).collect();
        for stream in &mut held {
            assert_eq!(status_line(stream), "HTTP/1.1 401 Unauthorized\r\n");
        }
        // Past the cap, a refused request waits for a free helper.
        let mut queued = stall();
        queued
            .set_read_timeout(Some(Duration::from_millis(300)))
            .expect("a read timeout");
        let mut byte = [0_u8];
        let early = queued.read(&mut byte);
        assert!(early.is_err(), "answered past the cap: {early:?}");
        // Reads go on while every helper is taken.
        let started = std::time::Instant::now();
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "{:?}",
            started.elapsed()
        );
        // A client that closes frees its helper for the queued request.
        held.pop()
            .expect("a held stream")
            .shutdown(Shutdown::Both)
            .expect("a shutdown");
        queued
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("a read timeout");
        assert_eq!(status_line(&mut queued), "HTTP/1.1 401 Unauthorized\r\n");
    }

    #[test]
    fn serve_helpers_queue_jobs_past_the_cap() {
        let helpers = Helpers::new(2);
        let (release, wait) = mpsc::channel::<()>();
        let wait = Arc::new(Mutex::new(wait));
        let (done, finished) = mpsc::channel();
        for job in 0..5 {
            let (wait, done) = (Arc::clone(&wait), done.clone());
            helpers.run(Box::new(move || {
                let _ = wait.lock().expect("the lock").recv();
                done.send(job).expect("a send");
            }));
        }
        assert_eq!(helpers.load(), (2, 3));
        for _ in 0..5 {
            release.send(()).expect("a send");
        }
        let mut jobs: Vec<i32> = (0..5)
            .map(|_| finished.recv_timeout(Duration::from_secs(3)).expect("a finished job"))
            .collect();
        jobs.sort_unstable();
        assert_eq!(jobs, [0, 1, 2, 3, 4]);
        thread::sleep(Duration::from_millis(50));
        assert_eq!(helpers.load(), (0, 0));
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
    fn serve_abandons_an_admitted_body_that_stalls() {
        let (port, token) = serve(Duration::from_millis(300));
        let head = format!(
            "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: 100000"
        );
        let stalled = send(port, &head, "\r\n{        ");
        thread::sleep(Duration::from_millis(100));
        // The mutation thread waits for the body; reads go on meanwhile.
        let started = std::time::Instant::now();
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert!(
            started.elapsed() < Duration::from_millis(150),
            "{:?}",
            started.elapsed()
        );
        thread::sleep(Duration::from_millis(400));
        stalled.shutdown(Shutdown::Write).expect("a shutdown");
        let timed_out = response(stalled);
        assert!(timed_out.starts_with("HTTP/1.1 408"), "{timed_out}");
        assert!(timed_out.contains("request_timeout"));
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
