//! Local HTTP adapter for the Arc Lightning web app.
//!
//! [`Server`] binds `127.0.0.1`, checks every request, serves the web app
//! page embedded from `apps/web/build/index.html` for any path outside
//! `/api`, and routes the read endpoints to a
//! [`Reads`] implementation that returns the CLI's JSON envelopes.
//!
//! Every request needs a `Host` header naming the server and the per-run
//! token in the [`TOKEN_HEADER`] header. The one exception is the first page
//! load, which carries the token in the [`TOKEN_QUERY`] query parameter of the
//! printed URL; the server accepts a query token once, and never on an API
//! path. Responses carry no CORS headers, and only GET is served.
//!
//! The page's Content-Security-Policy runs only the app's own inline script,
//! by hash, and lets it load images and fonts from itself or `data:` URLs and
//! connect only to this server. API responses allow nothing to load.
//!
//! | Path                        | CLI equivalent           |
//! | --------------------------- | ------------------------ |
//! | `/api/records`              | `arcl list`              |
//! | `/api/records/{id}`         | `arcl show <id>`         |
//! | `/api/ready`                | `arcl ready`             |
//! | `/api/tasks/{id}/explain`   | `arcl explain <task-id>` |
//! | `/api/tasks/{id}/context`   | `arcl context <task-id>` |
//! | `/api/work`                 | none; see below          |
//!
//! `/api/records` and `/api/ready` take the CLI's filter flags as query
//! parameters, such as `?kind=task&status=pending&status=parked`.
//!
//! `/api/work` answers the web app's Work list in one request: every task
//! with the readiness `arcl explain` reports and its effective phase ID, and
//! every phase, as `{"format_version": 2, "data": {"tasks": [{"task",
//! "readiness", "phase_id"}], "phases": [...]}}`. It takes no query
//! parameters.

use std::io;
use std::net::{Ipv4Addr, TcpListener};
use std::sync::LazyLock;

use thiserror::Error;
use tiny_http::{Header, Response};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The request header that carries the per-run token.
pub const TOKEN_HEADER: &str = "X-Arcl-Token";

/// The query parameter that carries the token on the first page load.
pub const TOKEN_QUERY: &str = "token";

/// The number of random bytes in a token, which is printed as hex.
const TOKEN_BYTES: usize = 32;

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
    /// Every task with its readiness and phase, and every phase. Holds
    /// whether the request carried query parameters, which it refuses.
    Work { has_query: bool },
    /// An `/api` path with no endpoint.
    UnknownApi,
    /// A web app page. The app routes on the client, so every page is
    /// `index.html`.
    Page,
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

/// A local server for the web app and its read endpoints.
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
    pub fn run(mut self, reads: &dyn Reads) -> Result<(), ServeError> {
        loop {
            let request = self.http.recv().map_err(ServeError::Accept)?;
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
                token: header(TOKEN_HEADER),
            };
            let answer = self.gate.answer(&incoming, reads);
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

/// The parts of a request the checks and routes read.
struct Incoming {
    method: String,
    url: String,
    host: Option<String>,
    token: Option<String>,
}

/// A response before it is written to the socket.
struct Answer {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
    allow_get: bool,
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
        if self.allow_get {
            headers.push(("Allow", "GET"));
        }
        headers
    }

    fn json(reply: Reply) -> Self {
        Self {
            status: reply.status,
            content_type: "application/json",
            body: reply.body.into_bytes(),
            allow_get: reply.status == 405,
            csp: API_CSP,
        }
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
    fn answer(&mut self, incoming: &Incoming, reads: &dyn Reads) -> Answer {
        let (path, query) = match split_url(&incoming.url) {
            Ok(parts) => parts,
            Err(message) => return Answer::json(Reply::invalid_request(&message)),
        };
        let query_token = token_in(&query);
        let route = route(&path, query);
        if let Err(reply) = self.admit(incoming, &route, query_token.as_deref()) {
            return Answer::json(reply);
        }
        match route {
            Route::List(query) => Answer::json(reads.list(&query)),
            Route::Show(id) => Answer::json(reads.show(&id)),
            Route::Ready(query) => Answer::json(reads.ready(&query)),
            Route::Explain(id) => Answer::json(reads.explain(&id)),
            Route::Context(id) => Answer::json(reads.context(&id)),
            Route::Work { has_query: true } => {
                Answer::json(Reply::invalid_request("`/api/work` takes no query parameters"))
            }
            Route::Work { has_query: false } => Answer::json(reads.work()),
            Route::UnknownApi => Answer::json(Reply::refusal(404, "not_found", "no endpoint has this path")),
            Route::Page => page(),
        }
    }

    /// Refuse a request with the wrong `Host`, without the token, or with a
    /// method other than GET.
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

        if incoming.method != "GET" {
            return Err(Reply::refusal(405, "method_not_allowed", "only GET is served"));
        }
        Ok(())
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
    /// Every task with its readiness and effective phase, and every phase,
    /// for the Work list.
    fn work(&self) -> Reply;
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
        ["api", "work"] => Route::Work { has_query: !query.is_empty() },
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
        allow_get: false,
        csp: PAGE_CSP.as_str(),
    }
}

#[cfg(test)]
mod tests {
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
        fn work(&self) -> Reply {
            Reply::success("work".to_owned())
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
            token: token.map(str::to_owned),
        }
    }

    fn error_code(answer: &Answer) -> String {
        let body: serde_json::Value = serde_json::from_slice(&answer.body).expect("a JSON body");
        body["error"]["code"].as_str().expect("an error code").to_owned()
    }

    #[test]
    fn serve_refuses_a_missing_token() {
        let answer = gate().answer(&get("/api/records", None), &Echo);
        assert_eq!(answer.status, 401);
        assert_eq!(error_code(&answer), "unauthorized");
    }

    #[test]
    fn serve_refuses_a_wrong_token() {
        let answer = gate().answer(&get("/api/records", Some("abc124")), &Echo);
        assert_eq!(answer.status, 401);
        let answer = gate().answer(&get("/", Some("abc")), &Echo);
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
            let answer = gate().answer(&incoming, &Echo);
            assert_eq!(answer.status, 403, "{host:?}");
            assert_eq!(error_code(&answer), "forbidden");
        }
        let incoming = Incoming { host: Some(format!("LOCALHOST:{PORT}")), ..get("/api/records", Some(TOKEN)) };
        assert_eq!(gate().answer(&incoming, &Echo).status, 200);
    }

    #[test]
    fn serve_accepts_the_query_token_for_the_first_page_load_only() {
        let mut gate = gate();
        let wrong = gate.answer(&get("/?token=nope", None), &Echo);
        assert_eq!(wrong.status, 401);

        let first = gate.answer(&get(&format!("/?token={TOKEN}"), None), &Echo);
        assert_eq!(first.status, 200);
        assert_eq!(first.content_type, "text/html; charset=utf-8");

        let again = gate.answer(&get(&format!("/?token={TOKEN}"), None), &Echo);
        assert_eq!(again.status, 401);
        let with_header = gate.answer(&get(&format!("/work?token={TOKEN}"), Some(TOKEN)), &Echo);
        assert_eq!(with_header.status, 401);
        assert_eq!(gate.answer(&get("/", Some(TOKEN)), &Echo).status, 200);
    }

    #[test]
    fn serve_never_accepts_a_query_token_on_the_api() {
        let answer = gate().answer(&get(&format!("/api/records?token={TOKEN}"), None), &Echo);
        assert_eq!(answer.status, 401);
        let answer = gate().answer(&get(&format!("/api/records/x?token={TOKEN}"), Some(TOKEN)), &Echo);
        assert_eq!(answer.status, 401);
    }

    #[test]
    fn serve_checks_the_host_and_token_on_the_work_endpoint() {
        assert_eq!(gate().answer(&get("/api/work", None), &Echo).status, 401);
        assert_eq!(gate().answer(&get("/api/work", Some("abc124")), &Echo).status, 401);
        let wrong_host = Incoming { host: Some("evil.example:4321".to_owned()), ..get("/api/work", Some(TOKEN)) };
        assert_eq!(gate().answer(&wrong_host, &Echo).status, 403);
        let query_token = gate().answer(&get(&format!("/api/work?token={TOKEN}"), None), &Echo);
        assert_eq!(query_token.status, 401);
        assert_eq!(gate().answer(&get("/api/work", Some(TOKEN)), &Echo).status, 200);
    }

    #[test]
    fn serve_answers_only_get() {
        let incoming = Incoming { method: "POST".to_owned(), ..get("/api/records", Some(TOKEN)) };
        let answer = gate().answer(&incoming, &Echo);
        assert_eq!(answer.status, 405);
        assert!(answer.headers().contains(&("Allow", "GET")));
        let anonymous = Incoming { method: "DELETE".to_owned(), ..get("/api/records", None) };
        assert_eq!(gate().answer(&anonymous, &Echo).status, 401);
    }

    #[test]
    fn serve_sends_no_cors_headers() {
        let answer = gate().answer(&get("/api/records", Some(TOKEN)), &Echo);
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
            let answer = gate().answer(&get(url, Some(TOKEN)), &Echo);
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
        let (status, refused) = body("/api/work?status=pending");
        assert_eq!(status, 400);
        assert!(refused.contains("invalid_request"), "{refused}");
        assert_eq!(body("/api/work/x").0, 404);
        assert_eq!(body("/api/nothing").0, 404);
        assert_eq!(body("/api/records/a%ZZ").0, 400);
    }

    #[test]
    fn serve_falls_back_to_the_app_page() {
        let answer = gate().answer(&get("/planning/anything", Some(TOKEN)), &Echo);
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
        let page = gate().answer(&get("/", Some(TOKEN)), &Echo);
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

        let api = gate().answer(&get("/api/records", Some(TOKEN)), &Echo);
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
}
