//! Local HTTP adapter for the Arc Lightning web app.
//!
//! [`Server`] binds `127.0.0.1`, checks every request, serves the web app
//! page embedded from `apps/web/build/index.html` for any path outside
//! `/api`, routes the read endpoints to a [`Reads`] implementation and the
//! task mutations to a [`Mutations`] implementation. Both answer with the
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
//! | Request                          | CLI equivalent           |
//! | -------------------------------- | ------------------------ |
//! | `GET /api/records`               | `arcl list`              |
//! | `GET /api/records/{id}`          | `arcl show <id>`         |
//! | `GET /api/ready`                 | `arcl ready`             |
//! | `GET /api/tasks/{id}/explain`    | `arcl explain <task-id>` |
//! | `GET /api/tasks/{id}/context`    | `arcl context <task-id>` |
//! | `POST /api/tasks`                | `arcl task create`       |
//! | `POST /api/tasks/{id}/update`    | `arcl task update`       |
//! | `POST /api/tasks/{id}/start`     | `arcl task start`        |
//! | `POST /api/tasks/{id}/park`      | `arcl task park`         |
//! | `POST /api/tasks/{id}/unpark`    | `arcl task unpark`       |
//! | `POST /api/tasks/{id}/handoff`   | `arcl task handoff`      |
//! | `POST /api/tasks/{id}/complete`  | `arcl task complete`     |
//!
//! `/api/records` and `/api/ready` take the CLI's filter flags as query
//! parameters, such as `?kind=task&status=pending&status=parked`. A mutation
//! takes a JSON object of the command's options, such as
//! `{"evidence": "Delivered by #41."}`; see [`NewTask`], [`TaskEdit`],
//! [`TaskStart`], [`Handoff`] and [`Completion`].

use std::io::{self, Read};
use std::net::{Ipv4Addr, TcpListener};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use thiserror::Error;
use tiny_http::{Header, Response};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The request header that carries the per-run token.
pub const TOKEN_HEADER: &str = "X-Arcl-Token";

/// The query parameter that carries the token on the first page load.
pub const TOKEN_QUERY: &str = "token";

/// The number of random bytes in a token, which is printed as hex.
const TOKEN_BYTES: usize = 32;

/// The largest request body a mutation accepts, in bytes.
const MAX_BODY_BYTES: usize = 1024 * 1024;

/// The page served when `apps/web` was not built before `arcl` was compiled.
const NOT_BUILT_PAGE: &str = include_str!("not_built.html");

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
    /// A task mutation, which needs POST and a matching `Origin`.
    Write(Write),
    /// An `/api` path with no endpoint.
    UnknownApi,
    /// A web app page. The app routes on the client, so every page is
    /// `index.html`.
    Page,
}

/// A task mutation and the task it names.
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
}

/// The JSON answer to an API request: an HTTP status and a body.
#[derive(Debug, Eq, PartialEq)]
pub struct Reply {
    status: u16,
    body: String,
}

impl Reply {
    /// A successful read whose body is the CLI's JSON envelope.
    pub fn success(body: String) -> Self {
        Self { status: 200, body }
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
        Self { status, body }
    }

    /// A malformed request, with the envelope the CLI prints for a usage
    /// error under `--json`.
    pub fn invalid_request(message: &str) -> Self {
        let body = serde_json::json!({
            "format_version": 1,
            "error": { "code": "invalid_request", "message": message, "exit_code": 2 },
        });
        Self { status: 400, body: body.to_string() }
    }

    /// The HTTP status code.
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// The JSON body.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// A request the server refused before it reached the application.
    fn refusal(status: u16, code: &str, message: &str) -> Self {
        let body = serde_json::json!({
            "format_version": 1,
            "error": { "code": code, "message": message },
        });
        Self { status, body: body.to_string() }
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

/// A local server for the web app and its endpoints.
pub struct Server {
    http: tiny_http::Server,
    gate: Gate,
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
        Ok(Self { http, gate: Gate::new(token, port) })
    }

    /// The port the server listens on.
    pub const fn port(&self) -> u16 {
        self.gate.port
    }

    /// The URL to open, with the token for the first page load.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/?{TOKEN_QUERY}={}", self.gate.port, self.gate.token)
    }

    /// Serve requests one at a time until the socket fails.
    pub fn run(mut self, reads: &dyn Reads, mutations: &dyn Mutations) -> Result<(), ServeError> {
        loop {
            let mut request = self.http.recv().map_err(ServeError::Accept)?;
            // One byte past the limit tells an oversized body from a full one.
            let mut body = Vec::new();
            let limit = u64::try_from(MAX_BODY_BYTES + 1).unwrap_or(u64::MAX);
            let body_read = request.as_reader().take(limit).read_to_end(&mut body).is_ok();
            let header = |name: &'static str| {
                request
                    .headers()
                    .iter()
                    .find(|header| header.field.equiv(name))
                    .map(|header| header.value.as_str().to_owned())
            };
            let incoming = Incoming {
                method: request.method().as_str().to_owned(),
                url: request.url().to_owned(),
                host: header("Host"),
                origin: header("Origin"),
                token: header(TOKEN_HEADER),
                body: body_read.then_some(body),
            };
            let answer = self.gate.answer(&incoming, reads, mutations);
            let headers = answer.headers();
            let mut response = Response::from_data(answer.body).with_status_code(answer.status);
            for (name, value) in headers {
                if let Ok(header) = Header::from_bytes(name, value) {
                    response = response.with_header(header);
                }
            }
            // A client that went away is not the server's failure.
            let _ = request.respond(response);
        }
    }
}

/// The body of a mutation that takes no options: an empty object.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct NoOptions {}

/// The parts of a request the checks and routes read.
struct Incoming {
    method: String,
    url: String,
    host: Option<String>,
    origin: Option<String>,
    token: Option<String>,
    /// The body, up to one byte past [`MAX_BODY_BYTES`], or `None` when it
    /// could not be read.
    body: Option<Vec<u8>>,
}

/// A response before it is written to the socket.
struct Answer {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    /// The method to name in an `Allow` header, on a 405.
    allow: Option<&'static str>,
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
        ];
        if let Some(method) = self.allow {
            headers.push(("Allow", method));
        }
        headers
    }

    fn json(reply: Reply) -> Self {
        Self { status: reply.status, content_type: "application/json", body: reply.body.into_bytes(), allow: None }
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

    /// Check `incoming`, then route it.
    fn answer(&mut self, incoming: &Incoming, reads: &dyn Reads, mutations: &dyn Mutations) -> Answer {
        let (path, query) = match split_url(&incoming.url) {
            Ok(parts) => parts,
            Err(message) => return Answer::json(Reply::invalid_request(&message)),
        };
        let query_token = token_in(&query);
        let route = route(&path, query);
        if let Err(reply) = self.admit(incoming, &route, query_token.as_deref()) {
            let allow = (reply.status == 405).then_some(if matches!(route, Route::Write(_)) { "POST" } else { "GET" });
            return Answer { allow, ..Answer::json(reply) };
        }
        match route {
            Route::List(query) => Answer::json(reads.list(&query)),
            Route::Show(id) => Answer::json(reads.show(&id)),
            Route::Ready(query) => Answer::json(reads.ready(&query)),
            Route::Explain(id) => Answer::json(reads.explain(&id)),
            Route::Context(id) => Answer::json(reads.context(&id)),
            Route::Write(write) => Answer::json(mutate(write, incoming.body.as_deref(), mutations)),
            Route::UnknownApi => Answer::json(Reply::refusal(404, "not_found", "no endpoint has this path")),
            Route::Page => page(),
        }
    }

    /// Refuse a request with the wrong `Host`, without the token, with the
    /// wrong method for its route, or a mutation whose `Origin` is missing or
    /// does not name the `Host`.
    fn admit(&mut self, incoming: &Incoming, route: &Route, query_token: Option<&str>) -> Result<(), Reply> {
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
}

/// The application's task mutations, each answering with the CLI's JSON
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
        ["api", "tasks"] => Route::Write(Write::Create),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    fn gate() -> Gate {
        Gate::new(TOKEN.to_owned(), PORT)
    }

    fn get(url: &str, token: Option<&str>) -> Incoming {
        Incoming {
            method: "GET".to_owned(),
            url: url.to_owned(),
            host: Some(format!("127.0.0.1:{PORT}")),
            origin: None,
            token: token.map(str::to_owned),
            body: Some(Vec::new()),
        }
    }

    /// A POST from the served origin with the token.
    fn post(url: &str, body: &str) -> Incoming {
        Incoming {
            method: "POST".to_owned(),
            origin: Some(format!("http://127.0.0.1:{PORT}")),
            body: Some(body.as_bytes().to_vec()),
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
        let answer = gate().answer(&get("/api/records", None), &Echo, &Echo);
        assert_eq!(answer.status, 401);
        assert_eq!(error_code(&answer), "unauthorized");
    }

    #[test]
    fn serve_refuses_a_wrong_token() {
        let answer = gate().answer(&get("/api/records", Some("abc124")), &Echo, &Echo);
        assert_eq!(answer.status, 401);
        let answer = gate().answer(&get("/", Some("abc")), &Echo, &Echo);
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
            let answer = gate().answer(&incoming, &Echo, &Echo);
            assert_eq!(answer.status, 403, "{host:?}");
            assert_eq!(error_code(&answer), "forbidden");
        }
        let incoming = Incoming { host: Some(format!("LOCALHOST:{PORT}")), ..get("/api/records", Some(TOKEN)) };
        assert_eq!(gate().answer(&incoming, &Echo, &Echo).status, 200);
    }

    #[test]
    fn serve_accepts_the_query_token_for_the_first_page_load_only() {
        let mut gate = gate();
        let wrong = gate.answer(&get("/?token=nope", None), &Echo, &Echo);
        assert_eq!(wrong.status, 401);

        let first = gate.answer(&get(&format!("/?token={TOKEN}"), None), &Echo, &Echo);
        assert_eq!(first.status, 200);
        assert_eq!(first.content_type, "text/html; charset=utf-8");

        let again = gate.answer(&get(&format!("/?token={TOKEN}"), None), &Echo, &Echo);
        assert_eq!(again.status, 401);
        let with_header = gate.answer(&get(&format!("/work?token={TOKEN}"), Some(TOKEN)), &Echo, &Echo);
        assert_eq!(with_header.status, 401);
        assert_eq!(gate.answer(&get("/", Some(TOKEN)), &Echo, &Echo).status, 200);
    }

    #[test]
    fn serve_never_accepts_a_query_token_on_the_api() {
        let answer = gate().answer(&get(&format!("/api/records?token={TOKEN}"), None), &Echo, &Echo);
        assert_eq!(answer.status, 401);
        let answer = gate().answer(
            &get(&format!("/api/records/x?token={TOKEN}"), Some(TOKEN)),
            &Echo,
            &Echo,
        );
        assert_eq!(answer.status, 401);
    }

    #[test]
    fn serve_answers_reads_only_on_get() {
        let incoming = post("/api/records", "");
        let answer = gate().answer(&incoming, &Echo, &Echo);
        assert_eq!(answer.status, 405);
        assert!(answer.headers().contains(&("Allow", "GET")));
        let anonymous = Incoming { method: "DELETE".to_owned(), ..get("/api/records", None) };
        assert_eq!(gate().answer(&anonymous, &Echo, &Echo).status, 401);
    }

    #[test]
    fn serve_answers_mutations_only_on_post() {
        for url in [
            "/api/tasks",
            "/api/tasks/arcl-t-1/start",
            "/api/tasks/arcl-t-1/complete",
        ] {
            for method in ["GET", "PUT", "DELETE", "OPTIONS"] {
                let incoming = Incoming { method: method.to_owned(), ..post(url, "{}") };
                let answer = gate().answer(&incoming, &Echo, &Echo);
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
            let incoming = Incoming { origin: origin.map(str::to_owned), ..post("/api/tasks/arcl-t-1/park", "") };
            let answer = gate().answer(&incoming, &Echo, &Echo);
            assert_eq!(answer.status, 403, "{origin:?}");
            assert_eq!(error_code(&answer), "forbidden");
        }
        let incoming = Incoming {
            host: Some(format!("localhost:{PORT}")),
            origin: Some(format!("http://localhost:{PORT}")),
            ..post("/api/tasks/arcl-t-1/park", "")
        };
        assert_eq!(
            text(gate().answer(&incoming, &Echo, &Echo)),
            (200, "park arcl-t-1".to_owned())
        );
    }

    #[test]
    fn serve_mutations_still_need_the_token_and_host() {
        let missing = Incoming { token: None, ..post("/api/tasks", r#"{"title":"T"}"#) };
        assert_eq!(gate().answer(&missing, &Echo, &Echo).status, 401);
        let host = Incoming { host: Some("evil.example".to_owned()), ..post("/api/tasks", r#"{"title":"T"}"#) };
        assert_eq!(gate().answer(&host, &Echo, &Echo).status, 403);
    }

    #[test]
    fn serve_routes_the_mutations() {
        let answer = |url: &str, body: &str| text(gate().answer(&post(url, body), &Echo, &Echo));
        assert_eq!(
            answer("/api/tasks", r#"{"title":"New","priority":"high"}"#),
            (
                200,
                r#"create NewTask { title: "New", body: None, priority: Some("high"), parent: None, spec: None, plan: None, phase: None }"#.to_owned()
            )
        );
        assert_eq!(
            answer("/api/tasks/arcl-t-1/update", r#"{"body":"Text"}"#),
            (
                200,
                r#"update arcl-t-1 TaskEdit { title: None, body: Some("Text"), priority: None }"#.to_owned()
            )
        );
        assert_eq!(
            answer("/api/tasks/arcl-t-1/start", ""),
            (200, "start arcl-t-1 TaskStart { force: false }".to_owned())
        );
        assert_eq!(
            answer("/api/tasks/arcl-t-1/park", "{}"),
            (200, "park arcl-t-1".to_owned())
        );
        assert_eq!(
            answer("/api/tasks/arcl-t-1/unpark", ""),
            (200, "unpark arcl-t-1".to_owned())
        );
        assert_eq!(
            answer("/api/tasks/arcl-t-1/handoff", r#"{"note":"Next"}"#),
            (200, r#"handoff arcl-t-1 Handoff { note: Some("Next") }"#.to_owned())
        );
        assert_eq!(
            answer("/api/tasks/arcl-t-1/complete", r#"{"evidence":"Done"}"#),
            (
                409,
                r#"complete arcl-t-1 Completion { evidence: Some("Done"), allow_open_children: false, with_subtasks: false }"#.to_owned()
            )
        );
        assert_eq!(answer("/api/tasks/arcl-t-1/delete", "").0, 404);
    }

    #[test]
    fn serve_refuses_malformed_mutation_bodies() {
        let answer = |url: &str, body: &[u8]| {
            let incoming = Incoming { body: Some(body.to_vec()), ..post(url, "") };
            gate().answer(&incoming, &Echo, &Echo)
        };
        for (url, body) in [
            ("/api/tasks", b"{}".as_slice()),
            ("/api/tasks", b"not json"),
            ("/api/tasks/arcl-t-1/park", br#"{"force":true}"#),
            ("/api/tasks/arcl-t-1/update", br#"{"spec_file":"/etc/passwd"}"#),
            ("/api/tasks/arcl-t-1/start", br#"{"force":"yes"}"#),
        ] {
            let refused = answer(url, body);
            assert_eq!(refused.status, 400, "{url} {body:?}");
            assert_eq!(error_code(&refused), "invalid_request");
        }
        let large = vec![b' '; MAX_BODY_BYTES + 1];
        assert_eq!(answer("/api/tasks/arcl-t-1/park", &large).status, 413);
        let unread = Incoming { body: None, ..post("/api/tasks/arcl-t-1/park", "") };
        assert_eq!(gate().answer(&unread, &Echo, &Echo).status, 400);
    }

    #[test]
    fn serve_sends_no_cors_headers() {
        let answer = gate().answer(&get("/api/records", Some(TOKEN)), &Echo, &Echo);
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
            let answer = gate().answer(&get(url, Some(TOKEN)), &Echo, &Echo);
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
        assert_eq!(body("/api/nothing").0, 404);
        assert_eq!(body("/api/records/a%ZZ").0, 400);
    }

    #[test]
    fn serve_falls_back_to_the_app_page() {
        let answer = gate().answer(&get("/planning/anything", Some(TOKEN)), &Echo, &Echo);
        assert_eq!(answer.status, 200);
        assert_eq!(answer.content_type, "text/html; charset=utf-8");
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
}
