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
//! mutation's body that does not arrive within [`BODY_DEADLINE`] gets 408,
//! and one that runs over 1 MiB gets 413. A connection that does not send a
//! request head within [`HEADER_DEADLINE`] is closed, idle keep-alive
//! connections included. So is one whose client reads its replies slower
//! than [`MIN_WRITE_RATE`] once [`WRITE_DEADLINE`] of slack is spent; no
//! single wait for the client passes [`WRITE_DEADLINE`]. A refusal before the
//! token is accepted (a bad `Host` or token, a bad request target, or 413),
//! and a refusal that leaves a mutation's body unread (408 or 413), carries
//! `Connection: close` and closes the connection. At most
//! [`MAX_CONNECTIONS`] are open at once; past that, new connections wait to
//! be accepted.
//!
//! [`Reads`] and [`Mutations`] stay synchronous. Reads run one at a time on
//! the thread that called [`Server::run`], and mutations one at a time on
//! their own thread, so a mutation that waits for a lock never holds up reads
//! and page loads. Ctrl-C stops accepting connections, lets open ones finish
//! for up to [`SHUTDOWN_GRACE`], and returns; a second Ctrl-C returns at
//! once. After Ctrl-C, mutations that have not started get 503 without
//! running, and one already running finishes before the server returns.
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
//!
//! [`apply`] runs a mutation named by one of [`operations`] with the same
//! JSON options, for `arcl web apply`, which applies the changes a claude.ai
//! artifact queued.

mod answer;
mod gate;
mod limits;
mod mutations;
mod routes;
mod server;
#[cfg(test)]
mod tests;

pub use answer::Reply;
pub use gate::{TOKEN_HEADER, TOKEN_QUERY};
pub use limits::{BODY_DEADLINE, HEADER_DEADLINE, MAX_CONNECTIONS, MIN_WRITE_RATE, SHUTDOWN_GRACE, WRITE_DEADLINE};
pub use mutations::{
    CapturePromotion, Completion, Handoff, Mutations, NewCapture, NewPlan, NewSpec, NewTask, PlanInput, TaskEdit,
    TaskStart, apply, operations,
};
pub use routes::Reads;
pub use server::{ServeError, Server};
