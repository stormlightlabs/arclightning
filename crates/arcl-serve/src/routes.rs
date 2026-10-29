//! Routing: which read or mutation a request's path names, and the read
//! operations behind it.

use serde::Deserialize;

use crate::answer::{Answer, Reply, page};

/// Where a request that passed the checks goes.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Route {
    /// `arcl list` with these filters.
    List(Vec<(String, String)>),
    /// `arcl show <id>`.
    Show(String),
    /// The workspace file of the record with this ID.
    Source(String),
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
    /// A GraphQL query, which needs POST but no `Origin`, since the schema
    /// has no mutations. The body is the [`GraphqlRequest`].
    Graphql,
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
pub(crate) enum Write {
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
    /// Replace the workspace file of the record with this ID.
    EditRecord(String),
}

/// The body of `POST /graphql`, as GraphQL clients send it. Other fields,
/// such as `extensions`, are ignored.
#[derive(Debug, Default, Deserialize, Eq, PartialEq)]
pub struct GraphqlRequest {
    /// The GraphQL document.
    pub query: String,
    /// The values of the document's variables: a JSON object, or `null`.
    #[serde(default)]
    pub variables: Option<serde_json::Value>,
    /// The operation to run when the document holds more than one.
    #[serde(default, rename = "operationName")]
    pub operation_name: Option<String>,
}

/// The application's read operations, each answering with the CLI's JSON
/// envelope for the same command.
pub trait Reads {
    /// `arcl list`, with the filter flags as `(name, value)` pairs.
    fn list(&self, query: &[(String, String)]) -> Reply;
    /// `arcl show <id>`.
    fn show(&self, id: &str) -> Reply;
    /// The workspace file of the record `id`, as `{"format_version": 1,
    /// "data": {"id", "kind", "path", "source", "revision"}}`. `source` is the
    /// file a workspace export writes for the record, and `revision` names
    /// its content for [`Mutations::edit_record`](crate::Mutations::edit_record).
    fn source(&self, id: &str) -> Reply;
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
    /// Run `request` as `arcl query` runs it, answering status 200 with the
    /// GraphQL response `arcl query` prints, `errors` included.
    fn graphql(&self, request: GraphqlRequest) -> Reply;
}

/// Answer an admitted read or page request. `body` is the request body,
/// which only [`Route::Graphql`] reads.
pub(crate) fn dispatch(route: Route, body: &[u8], reads: &dyn Reads) -> Answer {
    match route {
        Route::List(query) => Answer::json(reads.list(&query)),
        Route::Show(id) => Answer::json(reads.show(&id)),
        Route::Source(id) => Answer::json(reads.source(&id)),
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
        Route::Graphql => Answer::json(graphql(body, reads)),
        // `handle` sends mutations to the mutation thread.
        Route::Write(_) => Answer::json(Reply::internal("a mutation reached the read path")),
        Route::UnknownApi => Answer::json(Reply::refusal(404, "not_found", "no endpoint has this path")),
        Route::Page => page(),
    }
}

/// Parse `body` as a [`GraphqlRequest`] and run it. A body that is not one,
/// or whose `variables` is not an object, gets `invalid_request`.
fn graphql(body: &[u8], reads: &dyn Reads) -> Reply {
    match serde_json::from_slice::<GraphqlRequest>(body) {
        Ok(request)
            if request
                .variables
                .as_ref()
                .is_some_and(|variables| !variables.is_object()) =>
        {
            Reply::invalid_request("`variables` must be a JSON object")
        }
        Ok(request) => reads.graphql(request),
        Err(error) => Reply::invalid_request(&format!("the request body is not valid: {error}")),
    }
}

/// Split a request target into its decoded path and query pairs.
pub(crate) fn split_url(url: &str) -> Result<(String, Vec<(String, String)>), String> {
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
pub(crate) fn route(path: &str, query: Vec<(String, String)>) -> Route {
    let segments = path.trim_start_matches('/').split('/').collect::<Vec<_>>();
    match segments.as_slice() {
        ["api", "records"] => Route::List(query),
        ["api", "records", id] if !id.is_empty() => Route::Show((*id).to_owned()),
        ["api", "records", id, "source"] if !id.is_empty() => Route::Source((*id).to_owned()),
        ["api", "records", id, "edit"] if !id.is_empty() => Route::Write(Write::EditRecord((*id).to_owned())),
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
        ["graphql"] => Route::Graphql,
        _ => Route::Page,
    }
}
