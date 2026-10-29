use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tempfile::TempDir;

use super::*;
use arcl_repo::workspace::record_source;
use arcl_store::{RecordFilter, list_records};

/// A read a [`Reader`] makes at most once per query.
#[derive(Clone, Copy)]
pub(crate) enum Load {
    /// Rendering every record's workspace file.
    Sources,
    /// Loading the planning records task views read.
    Planning,
}

thread_local! {
    /// How many times this thread made each [`Load`]. A query runs on the
    /// thread that calls [`execute`].
    static LOADS: Cell<[usize; 2]> = const { Cell::new([0, 0]) };
}

/// Count one `load` on this thread.
pub(crate) fn count_load(load: Load) {
    LOADS.with(|loads| {
        let mut counts = loads.get();
        counts[load as usize] += 1;
        loads.set(counts);
    });
}

/// The records of the fixture project.
struct Fixture {
    _directory: TempDir,
    path: PathBuf,
    database: Database,
    spec: Spec,
    phase: Phase,
    blocker: PlanningTask,
    task: PlanningTask,
    gate: Gate,
    capture: Capture,
}

impl Fixture {
    /// A spec with a plan and a phase holding two tasks, where `blocker`
    /// blocks `task` and `task` has a gate and a note; a capture filed from
    /// `task`; a note; and a release holding the spec.
    fn new() -> Self {
        let directory = tempfile::tempdir().expect("temporary directory");
        let path = directory.path().join("arcl.db");
        let mut database = Database::open(&path).expect("database opens");
        let project_id = database.project().expect("project reads").id;
        let spec = database
            .create_spec("Reads".to_owned(), "Spec body".to_owned(), "It reads".to_owned())
            .expect("spec creates");
        let plan = database
            .create_plan(spec.id, "Roadmap".to_owned(), String::new())
            .expect("plan creates");
        let phase = database
            .create_phase(plan.id, "First".to_owned(), String::new(), 1)
            .expect("phase creates");
        let mut create = |title: &str, position: i64| {
            database
                .create_planning_task(PlanningTaskCreate {
                    project_id,
                    spec_id: None,
                    plan_id: None,
                    phase_id: Some(phase.id),
                    parent_id: None,
                    title: title.to_owned(),
                    body: format!("{title} body"),
                    priority: TaskPriority::High,
                    position,
                })
                .expect("task creates")
        };
        let blocker = create("Blocker", 1);
        let task = create("Blocked", 2);
        database
            .add_planning_dependency(task.id, blocker.id)
            .expect("dependency adds");
        let gate = database
            .add_gate(task.id, GateKind::Date, "2099-01-01")
            .expect("gate adds");
        database
            .add_task_note(task.id, Some(TaskNoteKind::Decision), "Use GraphQL")
            .expect("note adds");
        let capture = database
            .create_capture_from("Idea".to_owned(), "Capture body".to_owned(), Some(task.id))
            .expect("capture creates");
        database
            .create_note("Notes".to_owned(), String::new())
            .expect("note creates");
        let release = database
            .create_release("v1".to_owned(), String::new())
            .expect("release creates");
        database
            .add_release_membership(release.id, ReleaseMemberKind::Spec, spec.id.to_string())
            .expect("member adds");
        Self { _directory: directory, path, database, spec, phase, blocker, task, gate, capture }
    }

    /// Run `request` on a second connection, since [`execute`] takes the
    /// database it reads.
    fn execute(&self, request: Request) -> Value {
        let database = Database::open(&self.path).expect("database opens");
        serde_json::to_value(execute(database, request)).expect("response serializes")
    }

    /// Run `query` and return the whole response.
    fn run(&self, query: &str) -> Value {
        self.execute(Request::new(query))
    }

    /// Run `query` and return its `data`, failing on any error.
    fn data(&self, query: &str) -> Value {
        let response = self.run(query);
        assert!(response.get("errors").is_none(), "errors: {response}");
        response["data"].clone()
    }
}

/// A query whose fields nest `depth` deep: `tree`, then `children` down to
/// `id`.
fn nested_tree(depth: usize) -> String {
    let children = depth - 2;
    format!(
        "{{ tree {{ {}id{} }} }}",
        "children { ".repeat(children),
        " }".repeat(children)
    )
}

fn code(response: &Value) -> &str {
    response["errors"][0]["extensions"]["code"].as_str().unwrap_or_default()
}

#[test]
fn a_query_returns_only_the_fields_it_selects() {
    let fixture = Fixture::new();
    let data = fixture.data(&format!(
        r#"{{ task(id: "{}") {{ title priority }} }}"#,
        fixture.task.id
    ));
    assert_eq!(data, json!({ "task": { "title": "Blocked", "priority": "high" } }));
}

#[test]
fn ids_resolve_from_short_ids() {
    let fixture = Fixture::new();
    let full = fixture.task.id.to_string();
    let short = full[full.len() - 6..].to_owned();
    let data = fixture.data(&format!(
        r#"{{ record(id: "{short}") {{ ... on Record {{ id kind shortId }} }} }}"#
    ));
    assert_eq!(data["record"]["id"], json!(full));
    assert_eq!(data["record"]["kind"], json!("task"));
    assert!(full.ends_with(data["record"]["shortId"].as_str().expect("short ID")));
}

#[test]
fn record_takes_a_gate_id_as_arcl_show_does() {
    let fixture = Fixture::new();
    let full = fixture.gate.id.to_string();
    let short = fixture
        .database
        .short_ids()
        .expect("short IDs read")
        .short(&full)
        .to_owned();
    for id in [&full, &short] {
        let data = fixture.data(&format!(
            r#"{{ record(id: "{id}") {{ __typename ... on Gate {{ id kind target task {{ id }} }} ... on Record {{ title }} }} }}"#
        ));
        assert_eq!(
            data["record"],
            json!({
                "__typename": "Gate",
                "id": full,
                "kind": "date",
                "target": "2099-01-01",
                "task": { "id": fixture.task.id.to_string() },
            })
        );
    }
}

#[test]
fn a_query_renders_the_workspace_and_loads_the_planning_records_once() {
    let mut fixture = Fixture::new();
    let project_id = fixture.database.project().expect("project reads").id;
    let mut previous = fixture.task.id;
    for index in 0..150 {
        let task = fixture
            .database
            .create_planning_task(PlanningTaskCreate {
                project_id,
                spec_id: None,
                plan_id: None,
                phase_id: Some(fixture.phase.id),
                parent_id: None,
                title: format!("Task {index}"),
                body: String::new(),
                priority: TaskPriority::Normal,
                position: index + 3,
            })
            .expect("task creates");
        fixture
            .database
            .add_planning_dependency(task.id, previous)
            .expect("dependency adds");
        previous = task.id;
        fixture
            .database
            .create_capture(format!("Capture {index}"), String::new())
            .expect("capture creates");
    }
    let query = r#"{
        records { id source { revision } }
        tasks: records(kind: "task") {
            ... on Task { readiness { ready } blockers { task { id } } dependents { id } context { omittedNotes } }
        }
    }"#;
    LOADS.with(|loads| loads.set([0, 0]));
    let started = Instant::now();
    let data = fixture.data(query);
    let elapsed = started.elapsed();
    assert_eq!(LOADS.with(Cell::get), [1, 1]);
    assert_eq!(data["records"].as_array().expect("records").len(), 308);
    assert_eq!(data["tasks"].as_array().expect("tasks").len(), 152);
    // Before the reads were shared, this query took about 2 s in a debug build.
    assert!(elapsed < Duration::from_secs(2), "the query took {elapsed:?}");

    let task = fixture.task.id.to_string();
    let view = fixture
        .database
        .planning_task_view(fixture.task.id)
        .expect("view reads");
    let context = fixture
        .database
        .planning_context(fixture.task.id)
        .expect("context reads");
    let source = record_source(&fixture.database.connected_graph().expect("graph reads"), &task)
        .expect("source reads")
        .expect("source exists");
    let data = fixture.data(&format!(
        r#"{{ task(id: "{task}") {{
            source {{ revision content }}
            readiness {{ reasons {{ message }} }}
            dependents {{ id }}
            context {{ readiness {{ ready }} completionEvidence {{ task {{ id }} }} notes {{ text }} }}
        }} }}"#
    ));
    let task = &data["task"];
    assert_eq!(
        task["source"],
        json!({ "revision": source.revision, "content": source.content })
    );
    let messages = task["readiness"]["reasons"]
        .as_array()
        .expect("reasons")
        .iter()
        .map(|reason| reason["message"].as_str().expect("message").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(messages, view.readiness.messages());
    let dependents = view
        .dependents
        .iter()
        .map(|task| json!({ "id": task.id.to_string() }))
        .collect::<Vec<_>>();
    assert_eq!(task["dependents"], json!(dependents));
    assert_eq!(task["context"]["readiness"]["ready"], json!(context.readiness.ready));
    assert_eq!(
        task["context"]["notes"],
        json!(
            context
                .notes
                .iter()
                .map(|note| json!({ "text": note.text }))
                .collect::<Vec<_>>()
        )
    );
}

#[test]
fn tasks_carry_the_readiness_blockers_and_gates_explain_reports() {
    let fixture = Fixture::new();
    let data = fixture.data(&format!(
        r#"{{ task(id: "{}") {{
            readiness {{ ready blocked reasons {{ kind message blocker gate {{ id }} }} }}
            blockers {{ task {{ id }} completed }}
            gates {{ id kind target status }}
            notes {{ kind text }}
            phase {{ id }}
        }} }}"#,
        fixture.task.id
    ));
    let view = fixture
        .database
        .planning_task_view(fixture.task.id)
        .expect("view reads");
    let task = &data["task"];
    assert_eq!(task["readiness"]["ready"], json!(view.readiness.ready));
    assert_eq!(task["readiness"]["blocked"], json!(view.readiness.blocked));
    let reasons = task["readiness"]["reasons"].as_array().expect("reasons");
    let messages = reasons
        .iter()
        .map(|reason| reason["message"].as_str().expect("message").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(messages, view.readiness.messages());
    let kinds = reasons
        .iter()
        .map(|reason| reason["kind"].as_str().expect("kind"))
        .collect::<Vec<_>>();
    assert!(
        kinds.contains(&"direct_blocker") && kinds.contains(&"open_gate"),
        "{kinds:?}"
    );
    assert_eq!(
        task["blockers"],
        json!([{ "task": { "id": fixture.blocker.id.to_string() }, "completed": false }])
    );
    assert_eq!(
        task["gates"],
        json!([{ "id": fixture.gate.id.to_string(), "kind": "date", "target": "2099-01-01", "status": "open" }])
    );
    assert_eq!(task["notes"], json!([{ "kind": "decision", "text": "Use GraphQL" }]));
    assert_eq!(task["phase"]["id"], json!(fixture.phase.id.to_string()));
}

#[test]
fn records_and_ready_apply_the_list_and_ready_filters() {
    let fixture = Fixture::new();
    let graph = fixture.database.connected_graph().expect("graph reads");
    let data = fixture.data(&format!(
        r#"{{
            records(kind: "task", spec: "{}") {{ id }}
            all: records {{ kind }}
            ready(priority: ["high"]) {{ id }}
            release: records(release: "{}") {{ id }}
        }}"#,
        fixture.spec.id, graph.releases[0].id
    ));
    let filter =
        RecordFilter { kind: Some("task".to_owned()), spec_id: Some(fixture.spec.id), ..RecordFilter::default() };
    let expected = list_records(&graph, &filter)
        .expect("list reads")
        .into_iter()
        .map(|record| json!({ "id": record.id }))
        .collect::<Vec<_>>();
    assert_eq!(data["records"], json!(expected));
    assert_eq!(data["all"].as_array().expect("records").len(), 8);
    assert_eq!(data["ready"], json!([{ "id": fixture.blocker.id.to_string() }]));
    // As in `arcl list --release`, the release lists itself, then its members.
    assert_eq!(
        data["release"],
        json!([{ "id": graph.releases[0].id.to_string() }, { "id": fixture.spec.id.to_string() }])
    );
}

#[test]
fn a_bad_filter_or_missing_record_is_an_error_with_a_code() {
    let fixture = Fixture::new();
    let response = fixture.run(r#"{ records(kind: "widget") { id } }"#);
    assert_eq!(code(&response), "INVALID_REQUEST");
    let message = response["errors"][0]["message"].as_str().expect("message");
    assert!(message.contains("unknown kind `widget`"), "{message}");
    let response = fixture.run(r#"{ task(id: "arcl-t-00000000000000000000000000") { id } }"#);
    assert_eq!(code(&response), "NOT_FOUND");
}

#[test]
fn every_api_read_is_queryable() {
    let fixture = Fixture::new();
    let id = fixture.task.id.to_string();
    let data = fixture.data(&format!(
        r#"{{
            tree {{ id kind children {{ id children {{ id children {{ id ready blockedBy }} }} }} }}
            rooted: tree(root: "{id}") {{ id blockedBy }}
            work {{ tasks {{ task {{ id }} readiness {{ ready }} phaseId }} phases {{ id }} }}
            context(task: "{id}") {{ task {{ id }} blockers {{ task {{ id }} }} notes {{ text }} omittedNotes gates {{ id }} }}
            source(id: "{id}") {{ id kind path revision content }}
            explain: task(id: "{id}") {{ sourceTask {{ id }} }}
            capture(id: "{}") {{ sourceTask {{ id }} }}
        }}"#,
        fixture.capture.id
    ));
    let tree = fixture.database.record_tree(None, false).expect("tree reads");
    let tree_ids = data["tree"]
        .as_array()
        .expect("tree")
        .iter()
        .map(|node| node["id"].as_str().expect("id").to_owned())
        .collect::<Vec<_>>();
    assert_eq!(tree_ids, tree.iter().map(|node| node.id.clone()).collect::<Vec<_>>());
    let phase_tasks = &data["tree"][0]["children"][0]["children"][0]["children"];
    assert_eq!(phase_tasks[0]["ready"], json!(true));
    assert_eq!(phase_tasks[1]["blockedBy"], json!([fixture.blocker.id.to_string()]));
    assert_eq!(
        data["rooted"],
        json!([{ "id": id, "blockedBy": [fixture.blocker.id.to_string()] }])
    );

    let work = fixture.database.planning_work().expect("work reads");
    assert_eq!(data["work"]["tasks"].as_array().expect("tasks").len(), work.tasks.len());
    assert_eq!(data["work"]["tasks"][0]["phaseId"], json!(fixture.phase.id.to_string()));
    assert_eq!(data["work"]["phases"], json!([{ "id": fixture.phase.id.to_string() }]));

    let context = &data["context"];
    assert_eq!(context["task"]["id"], json!(id));
    assert_eq!(
        context["blockers"][0]["task"]["id"],
        json!(fixture.blocker.id.to_string())
    );
    assert_eq!(context["notes"], json!([{ "text": "Use GraphQL" }]));
    assert_eq!(context["omittedNotes"], json!(0));

    let graph = fixture.database.connected_graph().expect("graph reads");
    let source = record_source(&graph, &id)
        .expect("source reads")
        .expect("source exists");
    assert_eq!(data["source"]["path"], json!(format!("tasks/{id}.md")));
    assert_eq!(data["source"]["kind"], json!("task"));
    assert_eq!(data["source"]["revision"], json!(source.revision));
    assert_eq!(data["source"]["content"], json!(source.content));

    assert_eq!(data["explain"]["sourceTask"], Value::Null);
    assert_eq!(data["capture"]["sourceTask"]["id"], json!(id));
}

#[test]
fn variables_and_operation_names_pick_the_query() {
    let fixture = Fixture::new();
    let query = "query One($id: ID!) { task(id: $id) { title } } query Two { work { phases { title } } }";
    let request = Request::new(query)
        .variables(Variables::from_json(json!({ "id": fixture.task.id.to_string() })))
        .operation_name("One");
    assert_eq!(
        fixture.execute(request),
        json!({ "data": { "task": { "title": "Blocked" } } })
    );
}

#[test]
fn a_query_over_a_limit_is_refused_with_its_code() {
    let fixture = Fixture::new();
    let deep = fixture.run(&nested_tree(MAX_DEPTH + 1));
    assert_eq!(deep["data"], Value::Null);
    assert_eq!(code(&deep), QUERY_TOO_DEEP);
    assert!(fixture.run(&nested_tree(MAX_DEPTH)).get("errors").is_none());

    let fields = (0..=MAX_COMPLEXITY)
        .map(|index| format!("f{index}: work {{ __typename }}"))
        .collect::<Vec<_>>()
        .join(" ");
    let complex = fixture.run(&format!("{{ {fields} }}"));
    assert_eq!(complex["data"], Value::Null);
    assert_eq!(code(&complex), QUERY_TOO_COMPLEX);
}

#[test]
fn introspection_fits_the_limits() {
    let fixture = Fixture::new();
    let data = fixture.data(
        "{ __schema { queryType { name } types { name fields { name type { name kind ofType { name kind ofType { name kind } } } } } } }",
    );
    assert_eq!(data["__schema"]["queryType"]["name"], json!("Query"));
}

#[test]
fn the_checked_in_schema_matches_the_schema() {
    let checked_in = include_str!("../schema.graphql");
    assert!(
        checked_in.trim_end() == sdl().trim_end(),
        "crates/arcl-graphql/schema.graphql is out of date; regenerate it with `arcl query --schema`"
    );
}
