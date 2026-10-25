use std::io::{Read as _, Write as _};
use std::net::Ipv4Addr;
use std::net::{Shutdown, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use sha2::Digest;
use tokio::sync::oneshot;

use crate::answer::{API_CSP, Answer, RETRY_AFTER_SECONDS};
use crate::gate::{Gate, Incoming, TOKEN_BYTES, new_token};
use crate::limits::{Limits, MAX_BODY_BYTES};
use crate::mutations::mutate;
use crate::routes::{Route, dispatch};
use crate::*;

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
    fn tree(&self) -> Reply {
        Reply::success("tree".to_owned())
    }
    fn work(&self) -> Reply {
        Reply::success("work".to_owned())
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
    fn create_capture(&self, input: NewCapture) -> Reply {
        Reply::success(format!("capture {input:?}"))
    }
    fn promote_capture(&self, id: &str, input: CapturePromotion) -> Reply {
        Reply::success(format!("promote {id} {input:?}"))
    }
    fn discard_capture(&self, id: &str) -> Reply {
        Reply::success(format!("discard {id}"))
    }
    fn create_spec(&self, input: NewSpec) -> Reply {
        Reply::success(format!("spec {input:?}"))
    }
    fn create_plan(&self, input: NewPlan) -> Reply {
        Reply::success(format!("plan {input:?}"))
    }
    fn apply_plan(&self, id: &str, input: PlanInput) -> Reply {
        Reply::success(format!("apply {id} {input:?}"))
    }
}

fn gate() -> Gate {
    Gate::new(TOKEN.to_owned(), PORT)
}

/// Check and answer `incoming` as [`Server::run`] does, with an empty body.
fn answered(gate: &Gate, incoming: &Incoming) -> Answer {
    answered_with(gate, incoming, b"")
}

/// Check and answer `incoming` as [`Server::run`] does, with `body` as
/// the body read.
fn answered_with(gate: &Gate, incoming: &Incoming, body: &[u8]) -> Answer {
    match gate.admit(incoming) {
        Ok(Route::Write(write)) => Answer::json(mutate(write, body, &Echo)),
        Ok(route) => dispatch(route, &Echo),
        Err(answer) => answer,
    }
}

/// Serve on an ephemeral port on a background thread, and return the port
/// and the token.
fn serve(body_deadline: Duration) -> (u16, String) {
    serve_with(Limits { body: body_deadline, ..Limits::default() })
}

/// Serve as [`serve`] does, with these limits.
fn serve_with(limits: Limits) -> (u16, String) {
    let mut server = Server::bind(None).expect("a server");
    server.limits = limits;
    let port = server.port();
    let token = server.gate.token.clone();
    thread::spawn(move || server.run_until(&Echo, &Echo, std::future::pending()));
    (port, token)
}

/// Open a connection and send `head` and `body`, then leave it open.
fn send(port: u16, head: &str, body: &str) -> TcpStream {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("a read timeout");
    write!(stream, "{head}\r\nHost: 127.0.0.1:{port}\r\n").expect("a write");
    stream.write_all(body.as_bytes()).expect("a write");
    stream
}

/// Read a response to its end, or fail after the read timeout.
fn response(mut stream: TcpStream) -> String {
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("a response before the read timeout");
    response
}

/// Read the status line of a response on a connection that stays open.
fn status_line(stream: &mut TcpStream) -> String {
    let mut line = Vec::new();
    let mut byte = [0_u8];
    while !line.ends_with(b"\r\n") {
        stream
            .read_exact(&mut byte)
            .expect("a status line before the read timeout");
        line.push(byte[0]);
    }
    String::from_utf8(line).expect("UTF-8")
}

/// An authorized `GET /api/records` and its response.
fn authorized_get(port: u16, token: &str) -> String {
    let head = format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nConnection: close");
    response(send(port, &head, "\r\n"))
}

fn get(url: &str, token: Option<&str>) -> Incoming {
    Incoming {
        method: "GET".to_owned(),
        url: url.to_owned(),
        host: Some(format!("127.0.0.1:{PORT}")),
        origin: None,
        token: token.map(str::to_owned),
    }
}

/// A POST from the served origin with the token.
fn post(url: &str) -> Incoming {
    Incoming { method: "POST".to_owned(), origin: Some(format!("http://127.0.0.1:{PORT}")), ..get(url, Some(TOKEN)) }
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
    let answer = answered(&gate(), &get("/api/records", None));
    assert_eq!(answer.status, 401);
    assert_eq!(error_code(&answer), "unauthorized");
}

#[test]
fn serve_refuses_a_wrong_token() {
    let answer = answered(&gate(), &get("/api/records", Some("abc124")));
    assert_eq!(answer.status, 401);
    let answer = answered(&gate(), &get("/", Some("abc")));
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
        let answer = answered(&gate(), &incoming);
        assert_eq!(answer.status, 403, "{host:?}");
        assert_eq!(error_code(&answer), "forbidden");
    }
    let incoming = Incoming { host: Some(format!("LOCALHOST:{PORT}")), ..get("/api/records", Some(TOKEN)) };
    assert_eq!(answered(&gate(), &incoming).status, 200);
}

#[test]
fn serve_accepts_the_query_token_for_the_first_page_load_only() {
    let gate = gate();
    let wrong = answered(&gate, &get("/?token=nope", None));
    assert_eq!(wrong.status, 401);

    let first = answered(&gate, &get(&format!("/?token={TOKEN}"), None));
    assert_eq!(first.status, 200);
    assert_eq!(first.content_type, "text/html; charset=utf-8");

    let again = answered(&gate, &get(&format!("/?token={TOKEN}"), None));
    assert_eq!(again.status, 401);
    let with_header = answered(&gate, &get(&format!("/work?token={TOKEN}"), Some(TOKEN)));
    assert_eq!(with_header.status, 401);
    assert_eq!(answered(&gate, &get("/", Some(TOKEN))).status, 200);
}

#[test]
fn serve_never_accepts_a_query_token_on_the_api() {
    let answer = answered(&gate(), &get(&format!("/api/records?token={TOKEN}"), None));
    assert_eq!(answer.status, 401);
    let answer = answered(&gate(), &get(&format!("/api/records/x?token={TOKEN}"), Some(TOKEN)));
    assert_eq!(answer.status, 401);
}

#[test]
fn serve_checks_the_host_and_token_on_the_work_endpoint() {
    assert_eq!(answered(&gate(), &get("/api/work", None)).status, 401);
    assert_eq!(answered(&gate(), &get("/api/work", Some("abc124"))).status, 401);
    let wrong_host = Incoming { host: Some("evil.example:4321".to_owned()), ..get("/api/work", Some(TOKEN)) };
    assert_eq!(answered(&gate(), &wrong_host).status, 403);
    let query_token = answered(&gate(), &get(&format!("/api/work?token={TOKEN}"), None));
    assert_eq!(query_token.status, 401);
    assert_eq!(answered(&gate(), &get("/api/work", Some(TOKEN))).status, 200);
}

#[test]
fn serve_answers_reads_only_on_get() {
    let incoming = post("/api/records");
    let answer = answered(&gate(), &incoming);
    assert_eq!(answer.status, 405);
    assert!(answer.headers().contains(&("Allow", "GET")));
    let anonymous = Incoming { method: "DELETE".to_owned(), ..get("/api/records", None) };
    assert_eq!(answered(&gate(), &anonymous).status, 401);
}

#[test]
fn serve_answers_mutations_only_on_post() {
    for url in [
        "/api/tasks",
        "/api/tasks/arcl-t-1/start",
        "/api/tasks/arcl-t-1/complete",
        "/api/captures",
        "/api/captures/arcl-c-1/promote",
        "/api/captures/arcl-c-1/discard",
        "/api/specs",
        "/api/plans",
        "/api/plans/arcl-pl-1/apply",
    ] {
        for method in ["GET", "PUT", "DELETE", "OPTIONS"] {
            let incoming = Incoming { method: method.to_owned(), ..post(url) };
            let answer = answered(&gate(), &incoming);
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
        for url in [
            "/api/tasks/arcl-t-1/park",
            "/api/captures",
            "/api/captures/arcl-c-1/discard",
            "/api/specs",
        ] {
            let incoming = Incoming { origin: origin.map(str::to_owned), ..post(url) };
            let answer = answered_with(&gate(), &incoming, br#"{"title":"T"}"#);
            assert_eq!(answer.status, 403, "{origin:?} {url}");
            assert_eq!(error_code(&answer), "forbidden");
        }
    }
    let incoming = Incoming {
        host: Some(format!("localhost:{PORT}")),
        origin: Some(format!("http://localhost:{PORT}")),
        ..post("/api/tasks/arcl-t-1/park")
    };
    assert_eq!(text(answered(&gate(), &incoming)), (200, "park arcl-t-1".to_owned()));
}

#[test]
fn serve_mutations_still_need_the_token_and_host() {
    let missing = Incoming { token: None, ..post("/api/tasks") };
    assert_eq!(answered(&gate(), &missing).status, 401);
    let host = Incoming { host: Some("evil.example".to_owned()), ..post("/api/tasks") };
    assert_eq!(answered(&gate(), &host).status, 403);
}

#[test]
fn serve_routes_the_mutations() {
    let routed = |url: &str, body: &str| text(answered_with(&gate(), &post(url), body.as_bytes()));
    assert_eq!(
        routed("/api/tasks", r#"{"title":"New","priority":"high"}"#),
        (
            200,
            r#"create NewTask { title: "New", body: None, priority: Some("high"), parent: None, spec: None, plan: None, phase: None, blocked_by: [] }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/tasks/arcl-t-1/update", r#"{"body":"Text"}"#),
        (
            200,
            r#"update arcl-t-1 TaskEdit { title: None, body: Some("Text"), priority: None }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/tasks/arcl-t-1/start", ""),
        (200, "start arcl-t-1 TaskStart { force: false }".to_owned())
    );
    assert_eq!(
        routed("/api/tasks/arcl-t-1/park", "{}"),
        (200, "park arcl-t-1".to_owned())
    );
    assert_eq!(
        routed("/api/tasks/arcl-t-1/unpark", ""),
        (200, "unpark arcl-t-1".to_owned())
    );
    assert_eq!(
        routed("/api/tasks/arcl-t-1/handoff", r#"{"note":"Next"}"#),
        (200, r#"handoff arcl-t-1 Handoff { note: Some("Next") }"#.to_owned())
    );
    assert_eq!(
        routed("/api/tasks/arcl-t-1/complete", r#"{"evidence":"Done"}"#),
        (
            409,
            r#"complete arcl-t-1 Completion { evidence: Some("Done"), allow_open_children: false, with_subtasks: false }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/tasks", r#"{"title":"New","blocked_by":["arcl-t-2"]}"#),
        (
            200,
            r#"create NewTask { title: "New", body: None, priority: None, parent: None, spec: None, plan: None, phase: None, blocked_by: ["arcl-t-2"] }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/captures", r#"{"title":"Idea","body":"More"}"#),
        (
            200,
            r#"capture NewCapture { title: "Idea", body: Some("More") }"#.to_owned()
        )
    );
    assert_eq!(routed("/api/tasks/arcl-t-1/delete", "").0, 404);
}

#[test]
fn serve_routes_the_planning_mutations() {
    let routed = |url: &str, body: &str| text(answered_with(&gate(), &post(url), body.as_bytes()));
    assert_eq!(
        routed("/api/captures/arcl-c-1/promote", r#"{"to":"task","priority":"high","phase":"arcl-ph-1"}"#),
        (
            200,
            r#"promote arcl-c-1 CapturePromotion { to: "task", title: None, body: None, acceptance_criteria: None, spec: None, plan: None, phase: Some("arcl-ph-1"), parent: None, priority: Some("high") }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/captures/arcl-c-1/discard", ""),
        (200, "discard arcl-c-1".to_owned())
    );
    assert_eq!(
        routed("/api/specs", r#"{"title":"Spec","acceptance_criteria":"- works"}"#),
        (
            200,
            r#"spec NewSpec { title: "Spec", body: None, acceptance_criteria: Some("- works") }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/plans", r#"{"title":"Plan","spec":"arcl-s-1"}"#),
        (
            200,
            r#"plan NewPlan { title: "Plan", spec: "arcl-s-1", body: None }"#.to_owned()
        )
    );
    assert_eq!(
        routed("/api/plans/arcl-pl-1/apply", r#"{"document":"format-version = 1"}"#),
        (
            200,
            r#"apply arcl-pl-1 PlanInput { document: "format-version = 1" }"#.to_owned()
        )
    );
    assert_eq!(routed("/api/captures/arcl-c-1/update", "").0, 404);
    assert_eq!(routed("/api/plans/arcl-pl-1/check", "").0, 404);
    assert_eq!(routed("/api/specs/arcl-s-1", "").0, 404);
}

#[test]
fn serve_refuses_malformed_mutation_bodies() {
    let with_body = |url: &str, body: &[u8]| answered_with(&gate(), &post(url), body);
    for (url, body) in [
        ("/api/tasks", b"{}".as_slice()),
        ("/api/tasks", b"not json"),
        ("/api/tasks/arcl-t-1/park", br#"{"force":true}"#),
        ("/api/tasks/arcl-t-1/update", br#"{"spec_file":"/etc/passwd"}"#),
        ("/api/tasks/arcl-t-1/start", br#"{"force":"yes"}"#),
        ("/api/captures", b"{}"),
        ("/api/captures", br#"{"title":"Idea","priority":"high"}"#),
        ("/api/captures/arcl-c-1/promote", b"{}"),
        ("/api/captures/arcl-c-1/promote", br#"{"to":"task","position":3}"#),
        ("/api/captures/arcl-c-1/discard", br#"{"force":true}"#),
        (
            "/api/specs",
            br#"{"title":"S","acceptance_criteria_file":"/etc/passwd"}"#,
        ),
        ("/api/plans", br#"{"title":"P"}"#),
        (
            "/api/plans",
            br#"{"title":"P","spec":"arcl-s-1","input":"/etc/passwd"}"#,
        ),
        ("/api/plans/arcl-pl-1/apply", br#"{"file":"/etc/passwd"}"#),
        ("/api/plans/arcl-pl-1/apply", b""),
    ] {
        let refused = with_body(url, body);
        assert_eq!(refused.status, 400, "{url} {body:?}");
        assert_eq!(error_code(&refused), "invalid_request");
    }
}

#[test]
fn serve_sends_no_cors_headers() {
    let answer = answered(&gate(), &get("/api/records", Some(TOKEN)));
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
        let answer = answered(&gate(), &get(url, Some(TOKEN)));
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
    assert_eq!(body("/api/tree"), (200, "tree".to_owned()));
    assert_eq!(body("/api/tree?id=arcl-s-1").0, 400);
    let (status, refused) = body("/api/work?status=pending");
    assert_eq!(status, 400);
    assert!(refused.contains("invalid_request"), "{refused}");
    assert_eq!(body("/api/work/x").0, 404);
    assert_eq!(body("/api/nothing").0, 404);
    assert_eq!(body("/api/records/a%ZZ").0, 400);
}

#[test]
fn serve_falls_back_to_the_app_page() {
    let answer = answered(&gate(), &get("/planning/anything", Some(TOKEN)));
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
    let page = answered(&gate(), &get("/", Some(TOKEN)));
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

    let api = answered(&gate(), &get("/api/records", Some(TOKEN)));
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
            out.push(if i <= chunk.len() { char::from(ALPHABET[(word >> (18 - 6 * i)) as usize & 63]) } else { '=' });
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

#[test]
fn serve_refuses_a_stalled_body_without_waiting_for_it() {
    let (port, token) = serve(BODY_DEADLINE);
    // No token, a large declared body, ten bytes of it, then silence.
    let mut stalled = send(
        port,
        "POST /api/tasks HTTP/1.1\r\nContent-Length: 100000",
        "\r\n0123456789",
    );
    thread::sleep(Duration::from_millis(200));
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    assert_eq!(status_line(&mut stalled), "HTTP/1.1 401 Unauthorized\r\n");
}

#[test]
fn serve_answers_busy_with_retry_after() {
    let answer = Answer::json(Reply::busy(r#"{"error":{}}"#.to_owned()));
    assert_eq!(answer.status, 503);
    assert!(answer.headers().contains(&("Retry-After", RETRY_AFTER_SECONDS)));
    let plain = Answer::json(Reply::failure(1, String::new()));
    assert!(plain.headers().iter().all(|(name, _)| *name != "Retry-After"));
}

#[test]
fn serve_refuses_a_huge_declared_body_and_keeps_serving() {
    let (port, token) = serve(BODY_DEADLINE);
    let huge = "Content-Length: 1000000000000000";
    let origin = format!("Origin: http://127.0.0.1:{port}");
    for head in [
        format!("POST /api/tasks HTTP/1.1\r\n{huge}"),
        format!("POST /api/tasks HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\n{origin}\r\n{huge}"),
        format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\n{huge}"),
    ] {
        let refused = response(send(port, &head, "\r\n{\"title\":"));
        assert!(refused.starts_with("HTTP/1.1 413"), "{refused}");
        assert!(refused.contains("payload_too_large"), "{refused}");
        assert!(refused.contains(API_CSP), "{refused}");
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    }
}

#[test]
fn serve_refuses_a_chunked_body_over_the_limit() {
    let (port, token) = serve(BODY_DEADLINE);
    let head = format!(
        "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nTransfer-Encoding: chunked"
    );
    let over = MAX_BODY_BYTES + 1;
    let body = format!("\r\n{over:x}\r\n{}", " ".repeat(over));
    let mut stream = send(port, &head, &body);
    assert!(status_line(&mut stream).starts_with("HTTP/1.1 413"));
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
}

#[test]
fn serve_times_out_a_stalled_body_while_reads_stay_fast() {
    let (port, token) = serve(Duration::from_millis(300));
    let head = format!(
        "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: 100000"
    );
    let started = Instant::now();
    let stalled = send(port, &head, "\r\n{        ");
    thread::sleep(Duration::from_millis(50));
    // The body read waits on its own task; reads go on meanwhile.
    for _ in 0..3 {
        let read = Instant::now();
        assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
        assert!(read.elapsed() < Duration::from_millis(150), "{:?}", read.elapsed());
    }
    // The 408 arrives at the deadline, with the client still connected.
    let timed_out = response(stalled);
    assert!(timed_out.starts_with("HTTP/1.1 408"), "{timed_out}");
    assert!(timed_out.contains("request_timeout"));
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
}

#[test]
fn serve_answers_past_many_stalled_connections() {
    let (port, token) = serve(BODY_DEADLINE);
    let admitted = format!(
        "POST /api/tasks/arcl-t-1/park HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: 100000"
    );
    let refused = "POST /api/tasks HTTP/1.1\r\nContent-Length: 100000";
    let held: Vec<TcpStream> = (0..96)
        .map(|index| match index % 4 {
            // Connected, and nothing sent.
            0 => TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection"),
            // Half a request head.
            1 => {
                let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
                stream
                    .write_all(b"GET /api/records HTTP/1.1\r\nHost: ")
                    .expect("a write");
                stream
            }
            // An admitted mutation whose body stalls.
            2 => send(port, &admitted, "\r\n{"),
            // A refused request whose body stalls.
            _ => send(port, refused, "\r\n{"),
        })
        .collect();
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    drop(held);
}

/// Open a connection, write `request` pipelined `copies` times without
/// waiting, and never read the replies.
fn pipeline(port: u16, request: &str, copies: usize) -> TcpStream {
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
    stream.set_nonblocking(true).expect("a non-blocking socket");
    // A full send buffer or a closed connection ends the write early,
    // which is all the attack needs.
    let _ = stream.write_all(request.repeat(copies).as_bytes());
    stream
}

#[test]
fn serve_answers_past_clients_that_pipeline_refused_requests_and_never_read() {
    let (port, token) = serve(BODY_DEADLINE);
    let held: Vec<TcpStream> = (0..MAX_CONNECTIONS + 2)
        .map(|_| pipeline(port, "GET / HTTP/1.1\r\nHost: evil\r\n\r\n", 2000))
        .collect();
    thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    assert!(started.elapsed() < Duration::from_secs(1), "{:?}", started.elapsed());
    drop(held);
}

#[test]
fn serve_closes_a_connection_after_a_refusal_before_the_token() {
    let (port, token) = serve(BODY_DEADLINE);
    let huge = "Content-Length: 1000000000000000";
    for head in [
        "GET /api/records HTTP/1.1\r\nHost: evil".to_owned(),
        "GET /api/records HTTP/1.1".to_owned(),
        format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: wrong"),
        format!("GET /api/records?token={token} HTTP/1.1"),
        "GET /a%ZZ HTTP/1.1".to_owned(),
        format!("POST /api/tasks HTTP/1.1\r\n{huge}"),
    ] {
        // A second request follows on the same connection, unanswered.
        let mut stream = send(port, &head, "\r\n");
        let next = format!("GET /api/records HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\r\n");
        let _ = stream.write_all(next.as_bytes());
        let answered = response(stream);
        assert!(
            answered.contains("Connection: close") || answered.contains("connection: close"),
            "{answered}"
        );
        assert_eq!(answered.matches("HTTP/1.1 ").count(), 1, "{answered}");
    }
    // A refusal after the token keeps the connection open.
    let head = format!("POST /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nContent-Length: 0");
    let mut stream = send(port, &head, "\r\n");
    assert_eq!(status_line(&mut stream), "HTTP/1.1 405 Method Not Allowed\r\n");
    let next = format!(
        "GET /api/records HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(next.as_bytes()).expect("a write");
    assert!(response(stream).contains("HTTP/1.1 200 OK"));
}

#[test]
fn serve_closes_an_admitted_connection_that_never_reads() {
    let limits = Limits { connections: 1, write: Duration::from_millis(300), ..Limits::default() };
    let (port, token) = serve_with(limits);
    let request = format!("GET / HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{TOKEN_HEADER}: {token}\r\n\r\n");
    let held = pipeline(port, &request, 2000);
    thread::sleep(Duration::from_millis(100));
    let started = Instant::now();
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    drop(held);
}

#[test]
fn serve_closes_a_connection_that_stalls_its_head() {
    let (port, token) = serve_with(Limits { header: Duration::from_millis(300), ..Limits::default() });
    let mut idle = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
    let mut partial = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
    partial
        .write_all(b"GET /api/records HTTP/1.1\r\nHost: ")
        .expect("a write");
    let started = Instant::now();
    for stream in [&mut idle, &mut partial] {
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("a read timeout");
        let mut rest = Vec::new();
        stream.read_to_end(&mut rest).expect("the server closes the connection");
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(250) && elapsed < Duration::from_secs(2),
        "{elapsed:?}"
    );
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
}

#[test]
fn serve_caps_open_connections() {
    let (port, token) = serve_with(Limits { connections: 4, ..Limits::default() });
    let mut held: Vec<TcpStream> = (0..4)
        .map(|_| TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection"))
        .collect();
    thread::sleep(Duration::from_millis(100));
    // Past the cap, a connection waits to be accepted.
    let head = format!("GET /api/records HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nConnection: close");
    let mut queued = send(port, &head, "\r\n");
    queued
        .set_read_timeout(Some(Duration::from_millis(300)))
        .expect("a read timeout");
    let mut byte = [0_u8];
    let early = queued.read(&mut byte);
    assert!(early.is_err(), "answered past the cap: {early:?}");
    // A client that closes frees its place for the queued connection.
    held.pop()
        .expect("a held stream")
        .shutdown(Shutdown::Both)
        .expect("a shutdown");
    queued
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("a read timeout");
    assert_eq!(status_line(&mut queued), "HTTP/1.1 200 OK\r\n");
}

#[test]
fn serve_stops_when_shutdown_completes() {
    let server = Server::bind(None).expect("a server");
    let (port, token) = (server.port(), server.gate.token.clone());
    let (stop, stopped) = oneshot::channel::<()>();
    let (finished, done) = mpsc::channel();
    thread::spawn(move || {
        let result = server.run_until(&Echo, &Echo, async {
            let _ = stopped.await;
        });
        let _ = finished.send(result.is_ok());
    });
    let _idle = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).expect("a connection");
    assert!(authorized_get(port, &token).starts_with("HTTP/1.1 200"));
    stop.send(()).expect("a send");
    // The idle connection closes at once, so the grace period is not spent.
    let returned = done.recv_timeout(Duration::from_secs(2)).expect("run returns");
    assert!(returned);
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
}

#[test]
fn serve_reads_an_admitted_body_that_arrives() {
    let (port, token) = serve(BODY_DEADLINE);
    let body = format!("{{\"note\":\"{}\"}}", "n".repeat(4000));
    let head = format!(
        "POST /api/tasks/arcl-t-1/handoff HTTP/1.1\r\n{TOKEN_HEADER}: {token}\r\nOrigin: http://127.0.0.1:{port}\r\nContent-Length: {}\r\nConnection: close",
        body.len()
    );
    let answered = response(send(port, &head, &format!("\r\n{body}")));
    assert!(answered.starts_with("HTTP/1.1 200"), "{answered}");
    assert!(answered.contains("handoff arcl-t-1"));
}
