//! The checks every request passes before it is routed: `Host`, the token,
//! the method and `Origin`.

use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::request::Parts;

use crate::answer::{Answer, Reply};
use crate::routes::{Route, route, split_url};
use crate::server::ServeError;

/// The request header that carries the per-run token.
pub const TOKEN_HEADER: &str = "X-Arcl-Token";

/// The query parameter that carries the token on the first page load.
pub const TOKEN_QUERY: &str = "token";

/// The number of random bytes in a token, which is printed as hex.
pub(crate) const TOKEN_BYTES: usize = 32;

/// The checks every request passes before it is routed.
pub(crate) struct Gate {
    pub(crate) token: String,
    pub(crate) port: u16,
    /// Whether a page load has used the query token.
    first_load_used: AtomicBool,
}

impl Gate {
    pub(crate) fn new(token: String, port: u16) -> Self {
        Self { token, port, first_load_used: AtomicBool::new(false) }
    }

    /// Check the head of `incoming` and route it, or answer with the refusal.
    /// The body plays no part, so a refused request is never read. A refusal
    /// before the token is accepted closes the connection.
    pub(crate) fn admit(&self, incoming: &Incoming) -> Result<Route, Answer> {
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
            let allow =
                (reply.status() == 405).then_some(if matches!(route, Route::Write(_)) { "POST" } else { "GET" });
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

/// The parts of a request the checks and routes read.
pub(crate) struct Incoming {
    pub(crate) method: String,
    pub(crate) url: String,
    pub(crate) host: Option<String>,
    pub(crate) origin: Option<String>,
    pub(crate) token: Option<String>,
}

impl Incoming {
    /// The head of a request, with no body read. A header that is not
    /// visible ASCII counts as missing.
    pub(crate) fn head(parts: &Parts) -> Self {
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

/// Generate a token from the operating system's random source.
pub(crate) fn new_token() -> Result<String, ServeError> {
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
