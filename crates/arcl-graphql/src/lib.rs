//! The read-only GraphQL schema over a project's records, which `arcl query`
//! runs. Resolvers read through [`Database`] and the same read models as
//! `arcl show`, `list`, `ready`, `explain`, `context` and `tree`.

use std::collections::HashMap;
use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Mutex, OnceLock};
use std::task::{Context as PollContext, Poll, Wake, Waker};
use std::thread::{self, Thread};

use async_graphql::{EmptyMutation, EmptySubscription, ErrorExtensions, Schema, ServerError};

use arcl_core::prelude::*;
use arcl_repo::workspace::{RecordSource, record_sources};
use arcl_store::prelude::*;

mod query;
mod types;

pub use async_graphql::{Request, Response, Variables};
pub use query::QueryRoot;

/// How deep a query may nest fields, counting `{ tree { id } }` as 2. A
/// deeper query is refused with the error code [`QUERY_TOO_DEEP`].
pub const MAX_DEPTH: usize = 16;

/// The most fields a query may select, counting each field once. A larger
/// query is refused with the error code [`QUERY_TOO_COMPLEX`].
pub const MAX_COMPLEXITY: usize = 1000;

/// The `extensions.code` of a query nested deeper than [`MAX_DEPTH`].
pub const QUERY_TOO_DEEP: &str = "QUERY_TOO_DEEP";

/// The `extensions.code` of a query that selects more than
/// [`MAX_COMPLEXITY`] fields.
pub const QUERY_TOO_COMPLEX: &str = "QUERY_TOO_COMPLEX";

/// The schema type: queries only.
pub type ArclSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

/// The database a query reads, with the graph, short IDs, workspace files and
/// planning records it reads at most once per query.
struct Reader {
    database: Mutex<Database>,
    graph: OnceLock<ConnectedGraph>,
    short_ids: OnceLock<ShortIds>,
    sources: OnceLock<HashMap<String, RecordSource>>,
    planning: OnceLock<PlanningData>,
}

impl Reader {
    /// Run `read` against the database.
    fn read<T>(&self, read: impl FnOnce(&Database) -> Result<T, StorageError>) -> async_graphql::Result<T> {
        let database = self.database.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        read(&database).map_err(storage_error)
    }

    /// Every record and edge, read on first use.
    fn graph(&self) -> async_graphql::Result<&ConnectedGraph> {
        if let Some(graph) = self.graph.get() {
            return Ok(graph);
        }
        let graph = self.read(Database::connected_graph)?;
        Ok(self.graph.get_or_init(|| graph))
    }

    /// The short ID of `id`, as human output shows it.
    fn short_id<'a>(&self, id: &'a str) -> async_graphql::Result<&'a str> {
        if self.short_ids.get().is_none() {
            let ids = self.read(Database::short_ids)?;
            let _ = self.short_ids.set(ids);
        }
        Ok(self.short_ids.get().map_or(id, |ids| ids.short(id)))
    }

    /// The full ID `input` names, a full or short ID. `expected`, a full-ID
    /// prefix such as `arcl-t-`, narrows a short ID that names no kind.
    fn resolve(&self, input: &str, expected: Option<&str>) -> async_graphql::Result<String> {
        self.read(|database| database.resolve_record_id(input, expected))
    }

    /// Parse the full or short ID `input` as a `T`.
    fn parse<T: PrefixedId>(&self, input: &str) -> async_graphql::Result<T> {
        let id = self.resolve(input, Some(T::PREFIX))?;
        T::parse(&id).map_err(|error| coded(format!("invalid ID `{input}`: {error}"), "INVALID_REQUEST"))
    }

    /// The workspace file of the record `id`, or `None` when there is none.
    fn source(&self, id: &str) -> async_graphql::Result<Option<RecordSource>> {
        Ok(self.sources()?.get(id).cloned())
    }

    /// Every record's workspace file, by ID, rendered on first use.
    fn sources(&self) -> async_graphql::Result<&HashMap<String, RecordSource>> {
        if let Some(sources) = self.sources.get() {
            return Ok(sources);
        }
        #[cfg(test)]
        tests::count_load(tests::Load::Sources);
        let sources = record_sources(self.graph()?).map_err(|error| coded(error.to_string(), "INTERNAL"))?;
        Ok(self.sources.get_or_init(|| sources))
    }

    /// The `arcl explain` view of the task `id`.
    fn task_view(&self, id: TaskId) -> async_graphql::Result<PlanningTaskView> {
        let planning = self.planning()?;
        self.read(|database| database.planning_task_view_in(planning, id))
    }

    /// The project's planning records, loaded on first use.
    fn planning(&self) -> async_graphql::Result<&PlanningData> {
        if let Some(planning) = self.planning.get() {
            return Ok(planning);
        }
        #[cfg(test)]
        tests::count_load(tests::Load::Planning);
        let planning = self.read(Database::planning_data)?;
        Ok(self.planning.get_or_init(|| planning))
    }
}

/// The schema with the depth and complexity limits, ready to execute.
pub fn schema() -> ArclSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .limit_depth(MAX_DEPTH)
        .limit_complexity(MAX_COMPLEXITY)
        .finish()
}

/// The schema in SDL, as `arcl query --schema` prints it.
pub fn sdl() -> String {
    schema().sdl()
}

/// Run `request` against `database` on the calling thread. A refused or
/// failed query comes back as a response with `errors`; a limit refusal
/// carries [`QUERY_TOO_DEEP`] or [`QUERY_TOO_COMPLEX`] as its
/// `extensions.code`.
pub fn execute(database: Database, request: Request) -> Response {
    let reader = Reader {
        database: Mutex::new(database),
        graph: OnceLock::new(),
        short_ids: OnceLock::new(),
        sources: OnceLock::new(),
        planning: OnceLock::new(),
    };
    let mut response = block_on(schema().execute(request.data(reader)));
    for error in &mut response.errors {
        code_limit_error(error);
    }
    response
}

/// Give a limit refusal its `extensions.code`. async-graphql reports both
/// limits without one.
fn code_limit_error(error: &mut ServerError) {
    let code = match error.message.as_str() {
        "Query is nested too deep." => QUERY_TOO_DEEP,
        "Query is too complex." => QUERY_TOO_COMPLEX,
        _ => return,
    };
    error.extensions.get_or_insert_with(Default::default).set("code", code);
}

/// A storage failure as a GraphQL error, coded by its exit category:
/// `NOT_FOUND`, `INVALID_REQUEST`, `CONFLICT` or `INTERNAL`.
fn storage_error(error: StorageError) -> async_graphql::Error {
    let code = match error.exit_code() {
        5 => "NOT_FOUND",
        3 => "INVALID_REQUEST",
        4 => "CONFLICT",
        _ => "INTERNAL",
    };
    coded(error.to_string(), code)
}

/// A GraphQL error with `message` and `extensions.code`.
fn coded(message: String, code: &'static str) -> async_graphql::Error {
    async_graphql::Error::new(message).extend_with(|_, extensions| extensions.set("code", code))
}

/// Drive `future` to completion on this thread. The resolvers never wait on
/// I/O, so no async runtime is needed.
fn block_on<F: Future>(future: F) -> F::Output {
    struct Unpark(Thread);

    impl Wake for Unpark {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(Unpark(thread::current())));
    let mut context = PollContext::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
        thread::park();
    }
}

#[cfg(test)]
mod tests;
