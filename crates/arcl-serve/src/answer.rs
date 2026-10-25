//! Responses: the JSON [`Reply`] the application returns, the [`Answer`]
//! written to the socket, and the page with its Content-Security-Policy.

use std::sync::LazyLock;

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

/// The `Retry-After` value, in seconds, of a [`Reply::busy`] answer.
pub(crate) const RETRY_AFTER_SECONDS: &str = "1";

/// The Content-Security-Policy of every API response: a JSON body loads nothing.
pub(crate) const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'; base-uri 'none'; form-action 'none'";

/// The page served when `apps/web` was not built before `arcl` was compiled.
const NOT_BUILT_PAGE: &str = include_str!("not_built.html");

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

    /// A mutation whose body did not arrive within [`BODY_DEADLINE`](crate::BODY_DEADLINE).
    pub(crate) fn timed_out() -> Self {
        Self::refusal(408, "request_timeout", "the request body did not arrive in time")
    }

    /// A request whose body is, or is declared, over [`MAX_BODY_BYTES`](crate::limits::MAX_BODY_BYTES).
    pub(crate) fn too_large() -> Self {
        Self::refusal(413, "payload_too_large", "the request body is over 1 MiB")
    }

    /// A failure inside the server, such as a worker that panicked.
    pub(crate) fn internal(message: &str) -> Self {
        Self::refusal(500, "internal_error", message)
    }

    /// A request the server refused before it reached the application.
    pub(crate) fn refusal(status: u16, code: &str, message: &str) -> Self {
        let body = serde_json::json!({
            "format_version": 1,
            "error": { "code": code, "message": message },
        });
        Self { status, body: body.to_string(), retry: false }
    }
}

/// A response before it is written to the socket.
pub(crate) struct Answer {
    pub(crate) status: u16,
    pub(crate) content_type: &'static str,
    pub(crate) body: Vec<u8>,
    /// The method to name in an `Allow` header, on a 405.
    pub(crate) allow: Option<&'static str>,
    /// The `Retry-After` header's value, if any.
    pub(crate) retry_after: Option<&'static str>,
    /// The Content-Security-Policy header's value.
    pub(crate) csp: &'static str,
    /// Whether to send `Connection: close` and close the connection after
    /// this response.
    pub(crate) close: bool,
}

impl Answer {
    /// The headers every response carries, with no CORS headers among them.
    pub(crate) fn headers(&self) -> Vec<(&'static str, &'static str)> {
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

    pub(crate) fn json(reply: Reply) -> Self {
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
    pub(crate) fn closing(reply: Reply) -> Self {
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

/// Serve the web app page.
pub(crate) fn page() -> Answer {
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
