//! The checks every request passes before it is routed: `Host`, the token,
//! the method and `Origin`. `GET /healthz` skips them all.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};

use axum::http::request::Parts;

use crate::answer::{Answer, Reply};
use crate::routes::{Route, route, split_url};
use crate::server::ServeError;

/// The request header that carries the token.
pub const TOKEN_HEADER: &str = "X-Arcl-Token";

/// The query parameter that carries the token on a page load: the first one
/// for a minted token, any one for a supplied token.
pub const TOKEN_QUERY: &str = "token";

/// The fewest characters a token given to [`Server::bind_with`](crate::Server::bind_with)
/// may have. It may hold only ASCII letters and digits.
pub const MIN_TOKEN_CHARS: usize = 32;

/// The number of random bytes in a token, which is printed as hex.
pub(crate) const TOKEN_BYTES: usize = 32;

/// The checks every request passes before it is routed.
pub(crate) struct Gate {
    pub(crate) token: String,
    pub(crate) port: u16,
    /// Whether the server answers only `/api` and `/graphql`, as
    /// `arcl serve --api` does: no page, and no `Origin` needed on a change
    /// that sends none.
    pub(crate) api_only: bool,
    /// The `Host` values that name the server itself: `127.0.0.1:<port>`,
    /// `localhost:<port>` and the [`reachable`] address with the port.
    own_hosts: [String; 3],
    /// The `Host` values accepted beside [`Gate::own_hosts`], in lowercase,
    /// as [`allowed_host`] returns them.
    allowed_hosts: Vec<String>,
    /// Whether the token was supplied rather than minted, so every page load
    /// may carry it in [`TOKEN_QUERY`]. A minted token is printed in the URL,
    /// so the query accepts it for the first page load only.
    supplied: bool,
    /// Whether a page load has used the query token.
    first_load_used: AtomicBool,
}

impl Gate {
    /// The checks of a server listening on `address` and `port`, which also
    /// accepts `allowed_hosts` as `Host`.
    pub(crate) fn new(token: String, address: IpAddr, port: u16, api_only: bool, allowed_hosts: Vec<String>) -> Self {
        let own_hosts = [
            format!("127.0.0.1:{port}"),
            format!("localhost:{port}"),
            SocketAddr::new(reachable(address), port).to_string(),
        ];
        Self {
            token,
            port,
            api_only,
            own_hosts,
            allowed_hosts,
            supplied: false,
            first_load_used: AtomicBool::new(false),
        }
    }

    /// The same checks for a token the caller supplied, which every page
    /// load may carry in [`TOKEN_QUERY`]. The operator holds that token and
    /// it is never printed, so a second user or a reload needs it too.
    pub(crate) fn with_supplied_token(self) -> Self {
        Self { supplied: true, ..self }
    }

    /// Check the head of `incoming` and route it, or answer with the refusal.
    /// The body plays no part, so a refused request is never read. A refusal
    /// before the token is accepted closes the connection.
    ///
    /// `GET /healthz` passes with no `Host` or token check: a probe sends the
    /// pod's address as `Host`, and the answer reveals nothing.
    pub(crate) fn admit(&self, incoming: &Incoming) -> Result<Route, Answer> {
        let (path, query) =
            split_url(&incoming.url).map_err(|message| Answer::closing(Reply::invalid_request(&message)))?;
        let query_token = token_in(&query);
        let route = match route(&path, query) {
            Route::Page if self.api_only => Route::NotFound,
            Route::Health if incoming.method != "GET" => {
                let reply = Reply::refusal(405, "method_not_allowed", "this path takes GET only");
                return Err(Answer { allow: Some("GET"), ..Answer::json(reply) });
            }
            Route::Health => return Ok(Route::Health),
            route => route,
        };
        if self
            .authorize(incoming, &route, query_token.as_deref())
            .map_err(Answer::closing)?
        {
            return Ok(route);
        }
        if let Err(reply) = self.permit(incoming, &route) {
            let posts = matches!(route, Route::Write(_) | Route::Graphql);
            let allow = (reply.status() == 405).then_some(if posts { "POST" } else { "GET" });
            return Err(Answer { allow, ..Answer::json(reply) });
        }
        Ok(route)
    }

    /// Refuse a request with the wrong `Host` or without the token. Answers
    /// `true` for a page load with the token in [`TOKEN_QUERY`], which needs
    /// no further checks: any such load with a supplied token, and only the
    /// first with a minted one.
    fn authorize(&self, incoming: &Incoming, route: &Route, query_token: Option<&str>) -> Result<bool, Reply> {
        let host_matches = incoming.host.as_deref().is_some_and(|host| {
            self.own_hosts
                .iter()
                .chain(&self.allowed_hosts)
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
            let page_load = matches!(route, Route::Page) && incoming.method == "GET";
            // `swap` claims the first load, so two racing loads cannot both have it.
            if page_load
                && same_token(given, &self.token)
                && (self.supplied || !self.first_load_used.swap(true, Ordering::SeqCst))
            {
                return Ok(true);
            }
            let message = if self.api_only {
                format!("this server takes no token in the URL; send the token in the {TOKEN_HEADER} header")
            } else if self.supplied {
                format!("a token in the URL opens the page only; send the token in {TOKEN_HEADER}")
            } else {
                format!("the link's token works for the first page load only; send the token in {TOKEN_HEADER}")
            };
            return Err(Reply::refusal(401, "unauthorized", &message));
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
    /// whose `Origin` is neither `http://` and the `Host` nor `http://` or
    /// `https://` and an allowed host. The page mode also refuses a
    /// mutation without `Origin`; the API-only mode lets the token stand for
    /// it, since a browser page cannot send the token header cross-origin
    /// without a CORS preflight the server never answers. A GraphQL query
    /// takes POST but changes nothing, so it needs no `Origin`.
    fn permit(&self, incoming: &Incoming, route: &Route) -> Result<(), Reply> {
        match (route, incoming.method.as_str()) {
            // A path with no endpoint answers 404 to any method.
            (Route::NotFound, _) | (Route::Graphql, "POST") => Ok(()),
            (Route::Write(_), "POST") => {
                // `authorize` passed, so `Host` is present.
                let served = format!("http://{}", incoming.host.as_deref().unwrap_or_default());
                match incoming.origin.as_deref() {
                    None if self.api_only => Ok(()),
                    None => Err(Reply::refusal(
                        403,
                        "forbidden",
                        "a change needs an Origin header naming this server",
                    )),
                    Some(origin) if !origin.eq_ignore_ascii_case(&served) && !self.allows_origin(origin) => Err(
                        Reply::refusal(403, "forbidden", "the Origin header does not name this server"),
                    ),
                    Some(_) => Ok(()),
                }
            }
            (Route::Write(_) | Route::Graphql, _) => {
                Err(Reply::refusal(405, "method_not_allowed", "this path takes POST only"))
            }
            (_, "GET") => Ok(()),
            _ => Err(Reply::refusal(405, "method_not_allowed", "this path takes GET only")),
        }
    }

    /// Whether `origin` is `http://` or `https://` followed by an allowed
    /// host, as a gateway in front of the server sends it.
    fn allows_origin(&self, origin: &str) -> bool {
        let origin = origin.to_ascii_lowercase();
        let host = origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"));
        host.is_some_and(|host| self.allowed_hosts.iter().any(|allowed| allowed == host))
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

/// Check a `--allow-host` value, `host` or `host:port`, and return it in
/// lowercase. The host is a DNS name or an IPv4 address of ASCII letters,
/// digits, `.`, `-` and `_`, or an IPv6 address in brackets; the port is 1 to
/// 65535. Anything else, such as a scheme, a path or user information, is
/// refused with [`ServeError::AllowedHost`].
pub(crate) fn allowed_host(value: &str) -> Result<String, ServeError> {
    let refuse = || ServeError::AllowedHost(value.to_owned());
    let (host_valid, port) = match value.strip_prefix('[') {
        Some(rest) => {
            let (address, after) = rest.split_once(']').ok_or_else(refuse)?;
            let valid = !address.is_empty()
                && address
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b':' || byte == b'.');
            let port = match after {
                "" => None,
                after => Some(after.strip_prefix(':').ok_or_else(refuse)?),
            };
            (valid, port)
        }
        None => {
            let (host, port) = match value.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (value, None),
            };
            let valid = !host.is_empty()
                && host.len() <= 253
                && host
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
            (valid, port)
        }
    };
    let port_valid = port
        .is_none_or(|port| port.bytes().all(|byte| byte.is_ascii_digit()) && matches!(port.parse::<u16>(), Ok(1..)));
    if host_valid && port_valid { Ok(value.to_ascii_lowercase()) } else { Err(refuse()) }
}

/// The address a client on this machine dials to reach a server listening on
/// `address`: `127.0.0.1` for `0.0.0.0`, `::1` for `::`, and `address`
/// itself otherwise.
pub(crate) const fn reachable(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V4(v4) if v4.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(v6) if v6.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
        address => address,
    }
}

/// Generate a token from the operating system's random source.
pub(crate) fn new_token() -> Result<String, ServeError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(ServeError::Token)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Refuse a token that is shorter than [`MIN_TOKEN_CHARS`] or holds anything
/// but ASCII letters and digits.
pub(crate) fn check_token(token: &str) -> Result<(), ServeError> {
    if token.len() >= MIN_TOKEN_CHARS && token.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        Ok(())
    } else {
        Err(ServeError::WeakToken)
    }
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
