//! HTTP adapter for the Arc Lightning web app.
//!
//! [`Server`] binds `127.0.0.1`, or the address [`ServeOptions`] names,
//! checks every request, serves the web app page embedded from
//! `apps/web/build/index.html` for any path outside `/api`, `/graphql` and
//! `/healthz`, routes the read endpoints and GraphQL queries to a [`Reads`]
//! implementation and the mutations to a [`Mutations`] implementation. Both
//! answer with the CLI's JSON output.
//!
//! Every request needs a `Host` header naming the server and the token in
//! the [`TOKEN_HEADER`] header. The one exception is a page load, which may
//! carry the token in the [`TOKEN_QUERY`] query parameter instead, never on
//! an API path. A minted token is printed in the URL, so the server accepts
//! it in the query for the first page load only. A token the caller
//! supplies is never printed, so the query accepts it on every page load,
//! for a reload or a second user. The page is sent with
//! `Referrer-Policy: no-referrer`, so the URL's token does not leave in a
//! `Referer`. The `Host` is `127.0.0.1:<port>`, `localhost:<port>`, the bound
//! address and port (`[::1]:<port>` for `::`, and none for `0.0.0.0`), or
//! one of [`ServeOptions::allowed_hosts`]. A mutation also needs an
//! `Origin` header naming the server: `http://` and the `Host`, or `http://`
//! or `https://` and an allowed host, as a gateway that terminates TLS sends
//! it. Reads take
//! GET and mutations take POST; any other method is refused, so GET never
//! changes state. `/graphql` takes POST only, and needs no `Origin`, since
//! the schema has no mutations. Responses carry no CORS headers.
//!
//! `GET /healthz` answers 200 `ok` with no `Host` or token check, for a
//! liveness probe, which sends the pod's address as `Host`. `GET
//! /api/version` answers `{"format_version": 1, "data": {"version"}}` with
//! the crate version, after the usual checks.
//!
//! [`Server::bind_with`] refuses to mint a token for an address off
//! loopback, since the CLI prints a minted token to its log. Such a server
//! needs a token the caller supplies.
//!
//! [`Server::bind_api`] makes a server that answers only `/api`, `/graphql`
//! and `/healthz`, for scripts and other local tools. Any other path gets 404
//! once the `Host` and token checks pass, and there is no first page load, so
//! no query token. The caller may choose the token, which needs at least
//! [`MIN_TOKEN_CHARS`] characters, all ASCII letters and digits. A mutation
//! needs a matching `Origin` only when it sends one: a mismatched `Origin` is
//! refused, and a request without one runs on the token alone. A
//! browser page on another origin cannot send the [`TOKEN_HEADER`] header
//! without a CORS preflight, and the server refuses every preflight without
//! CORS headers, so the token check stands in for `Origin`.
//!
//! The server runs axum on hyper and a single-threaded tokio runtime. It
//! checks a request's head before it reads any of the body, and reads a body
//! only for a mutation or GraphQL query that passed every check. A request
//! that declares a body over 1 MiB gets 413 before any check, and its body is
//! never read. A body that does not arrive within [`BODY_DEADLINE`] gets 408,
//! and one that runs over 1 MiB gets 413. A connection that does not send a
//! request head within [`HEADER_DEADLINE`] is closed, idle keep-alive
//! connections included. So is one whose client reads its replies slower
//! than [`MIN_WRITE_RATE`] once [`WRITE_DEADLINE`] of slack is spent; no
//! single wait for the client passes [`WRITE_DEADLINE`]. A refusal before the
//! token is accepted (a bad `Host` or token, a bad request target, or 413),
//! and a refusal that leaves a body unread (408 or 413), carries
//! `Connection: close` and closes the connection. At most
//! [`MAX_CONNECTIONS`] are open at once; past that, new connections wait to
//! be accepted.
//!
//! [`Reads`] and [`Mutations`] stay synchronous. Reads, GraphQL queries
//! included, run one at a time on the thread that called [`Server::run`],
//! and mutations one at a time on their own thread, so a mutation that waits
//! for a lock never holds up reads and page loads. A read whose client
//! closed its connection while it waited in line is dropped without running.
//! Ctrl-C stops accepting connections, lets open ones finish for up to
//! [`SHUTDOWN_GRACE`], and returns; a second Ctrl-C ends the grace period.
//! After Ctrl-C, reads and mutations that have not started get 503 without
//! running, and the read or mutation already running finishes before the
//! server returns.
//!
//! The page's Content-Security-Policy runs only the app's own inline script,
//! by hash, and lets it load images and fonts from itself or `data:` URLs and
//! connect only to this server. API responses allow nothing to load.
//!
//! | Request                          | CLI equivalent           |
//! | -------------------------------- | ------------------------ |
//! | `GET /api/records`               | `arcl list`              |
//! | `GET /api/records/{id}`          | `arcl show <id>`         |
//! | `GET /api/records/{id}/source`   | none; see below          |
//! | `GET /api/ready`                 | `arcl ready`             |
//! | `GET /api/tasks/{id}/explain`    | `arcl explain <task-id>` |
//! | `GET /api/tasks/{id}/context`    | `arcl context <task-id>` |
//! | `GET /api/tree`                  | `arcl tree`              |
//! | `GET /api/work`                  | none; see below          |
//! | `GET /api/version`               | `arcl --version`         |
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
//! | `POST /api/records/{id}/edit`    | none; see below          |
//! | `POST /graphql`                  | `arcl query`             |
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
//! `/api/records/{id}/source` answers the record's workspace file, the bytes
//! a workspace export writes, with a `revision` naming them. `POST
//! /api/records/{id}/edit` takes `{"source", "revision"}` (see
//! [`RecordSourceEdit`]) and replaces the record with the file when its
//! revision still matches, after the checks a workspace import runs on a
//! file. It refuses a change to a field only a command changes, such as a
//! task's `status`, as `arcl_repo::workspace::edit_record` lists. The record's
//! ID is the only path the client names.
//!
//! `POST /graphql` takes `{"query", "variables", "operationName"}` (see
//! [`GraphqlRequest`]) and answers status 200 with the GraphQL response
//! `arcl query` prints for the same query: `data`, and `errors` when there
//! are any, a query over a limit included. `arcl query` refuses a query
//! document over 64 KiB with the code `QUERY_TOO_LARGE` before parsing it,
//! so `/graphql` does too. A body that is not such an object,
//! or whose `variables` is not an object, gets `invalid_request` with status
//! 400, as a malformed mutation body does.
//!
//! A mutation takes a JSON object of the command's options, such as
//! `{"evidence": "Delivered by #41."}`; see [`NewTask`], [`TaskEdit`],
//! [`TaskStart`], [`Handoff`], [`Completion`], [`NewCapture`],
//! [`CapturePromotion`], [`NewSpec`], [`NewPlan`] and [`PlanInput`]. A plan
//! applies from the TOML document `arcl plan apply --file` reads, sent as the
//! `document` string.
//!
//! [`apply`] runs a mutation named by one of [`operations`] with the same
//! JSON options and the same 1 MiB limit on them, for `arcl web apply`,
//! which applies the changes a claude.ai artifact queued. Its
//! `update_source` operation is `POST /api/records/{id}/edit`, so a queued
//! raw edit gets the same checks and the same conflict error for a stale
//! revision.

mod answer;
mod gate;
mod limits;
mod mutations;
mod routes;
mod server;
#[cfg(test)]
mod tests;

pub use answer::Reply;
pub use gate::{MIN_TOKEN_CHARS, TOKEN_HEADER, TOKEN_QUERY};
pub use limits::{BODY_DEADLINE, HEADER_DEADLINE, MAX_CONNECTIONS, MIN_WRITE_RATE, SHUTDOWN_GRACE, WRITE_DEADLINE};
pub use mutations::{
    CapturePromotion, Completion, Handoff, Mutations, NewCapture, NewPlan, NewSpec, NewTask, PlanInput,
    RecordSourceEdit, TaskEdit, TaskStart, apply, operations,
};
pub use routes::{GraphqlRequest, Reads};
pub use server::{ServeError, ServeOptions, Server};
