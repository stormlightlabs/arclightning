import { describe, expect, it } from "vitest";
import { DataError, dataSource, latestRead, serveSource, setDataSource, type DataSource } from "./index";

const phase = (id: string, position: number) => ({ id, project_id: "pj", plan_id: "pl", title: `Milestone ${position}`, body: "", status: "open", position });

function item(id: string, phaseId: string | null) {
  return {
    task: { id, title: `Task ${id}`, status: "pending", priority: "normal", phase_id: null },
    readiness: { ready: true, blocked: false, reasons: [] },
    phase_id: phaseId,
  };
}

/** A fetch that answers from `routes` and records each request. */
function fakeFetch(routes: Record<string, { status?: number; body: unknown }>) {
  const calls: { path: string; headers: Record<string, string>; method?: string; body?: unknown; signal?: AbortSignal | null }[] =
    [];
  const fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input);
    const call: (typeof calls)[number] = { path, headers: (init?.headers ?? {}) as Record<string, string> };
    if (init?.method) call.method = init.method;
    if (init?.body !== undefined) call.body = JSON.parse(String(init.body));
    if (init?.signal !== undefined) call.signal = init.signal;
    calls.push(call);
    const route = routes[path];
    if (!route) return new Response("{}", { status: 404 });
    return new Response(JSON.stringify(route.body), { status: route.status ?? 200 });
  }) as typeof globalThis.fetch;
  return { fetch, calls };
}

const listing = (records: { id: string; kind: string; status: string; title: string }[]) => ({ format_version: 1, data: { records } });

/** A fetch that never answers and rejects when its signal aborts, as the browser's does. */
function hangingFetch() {
  const signals: AbortSignal[] = [];
  const fetch = ((_input: RequestInfo | URL, init?: RequestInit) =>
    new Promise<Response>((_resolve, reject) => {
      const signal = init?.signal;
      if (!signal) return;
      signals.push(signal);
      signal.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
    })) as typeof globalThis.fetch;
  return { fetch, signals };
}

describe("serveSource", () => {
  it("builds the Work list from one request, with the token on it", async () => {
    const { fetch, calls } = fakeFetch({
      "/api/work": {
        body: { format_version: 2, data: { tasks: [item("t-2", "ph-7"), item("t-1", null)], phases: [phase("ph-7", 7), phase("ph-8", 8)] } },
      },
    });

    const work = await serveSource({ fetch, headers: () => ({ "X-Arcl-Token": "abc" }) }).work();

    expect(work.milestones).toEqual([
      { id: "ph-7", plan_id: "pl", title: "Milestone 7", status: "open", position: 7 },
      { id: "ph-8", plan_id: "pl", title: "Milestone 8", status: "open", position: 8 },
    ]);
    expect(work.items.map((entry) => [entry.task.id, entry.milestone?.id ?? null])).toEqual([
      ["t-2", "ph-7"],
      ["t-1", null],
    ]);
    expect(work.items[0].milestone).toEqual({ id: "ph-7", plan_id: "pl", title: "Milestone 7", status: "open", position: 7 });
    expect(calls.map((call) => call.path)).toEqual(["/api/work"]);
    expect(calls[0].headers["X-Arcl-Token"]).toBe("abc");
  });

  it("raises the CLI's error code and message", async () => {
    const { fetch } = fakeFetch({
      "/api/work": { status: 401, body: { format_version: 1, error: { code: "unauthorized", message: "the X-Arcl-Token header is missing" } } },
    });
    const failure = await serveSource({ fetch, headers: () => ({}) }).work().catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(DataError);
    expect(failure).toMatchObject({ code: "unauthorized", message: "the X-Arcl-Token header is missing" });
  });

  it("stops the request when its signal aborts", async () => {
    const { fetch, signals } = hangingFetch();
    const controller = new AbortController();
    const pending = serveSource({ fetch, headers: () => ({}) }).work(controller.signal);
    controller.abort();
    await expect(pending).rejects.toMatchObject({ name: "AbortError" });
    expect(signals).toEqual([controller.signal]);
  });
});

describe("latestRead", () => {
  it("aborts the read in flight on restart and on stop, and reports only the newest", async () => {
    const { fetch, signals } = hangingFetch();
    const source = serveSource({ fetch, headers: () => ({}) });
    const reported: string[] = [];
    const read = latestRead((signal) => source.work(signal), {
      loaded: () => reported.push("loaded"),
      failed: (error) => reported.push(`failed: ${String(error)}`),
    });

    read.start();
    read.start(); // Retry
    expect(signals.map((signal) => signal.aborted)).toEqual([true, false]);
    read.stop(); // unmount
    expect(signals.map((signal) => signal.aborted)).toEqual([true, true]);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(reported).toEqual([]);
  });

  it("reports the newest read's value or failure", async () => {
    const reported: unknown[] = [];
    const report = { loaded: (value: unknown) => reported.push(value), failed: (error: unknown) => reported.push(error) };
    latestRead(() => Promise.resolve("list"), report).start();
    latestRead(() => Promise.reject(new Error("database is locked")), report).start();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(reported).toEqual(["list", new Error("database is locked")]);
  });
});

describe("serveSource.task", () => {
  it("reads the task's show envelope, escaping the ID, with the token", async () => {
    const { task, readiness } = item("t 1", null);
    const detail = {
      task,
      readiness,
      phase: null,
      blockers: [],
      inherited_blockers: [],
      dependents: [],
      children: [],
      ancestors: [],
      spec: null,
      plan: null,
    };
    const { fetch, calls } = fakeFetch({ "/api/records/t%201": { body: { format_version: 2, data: detail } } });
    const signal = new AbortController().signal;
    const loaded = await serveSource({ fetch, headers: () => ({ "X-Arcl-Token": "abc" }) }).task("t 1", signal);
    expect(loaded).toEqual(detail);
    expect(calls).toEqual([{ path: "/api/records/t%201", headers: { "X-Arcl-Token": "abc" }, signal }]);
  });

  it("refuses a record that is not a task", async () => {
    const { fetch } = fakeFetch({ "/api/records/ph-7": { body: { format_version: 1, data: { phase } } } });
    const failure = await serveSource({ fetch, headers: () => ({}) }).task("ph-7").catch((error: unknown) => error);
    expect(failure).toMatchObject({ code: "not_a_task", message: "`ph-7` is not a task" });
  });

  it("raises not_found for a missing task", async () => {
    const { fetch } = fakeFetch({
      "/api/records/t-9": { status: 404, body: { format_version: 1, error: { code: "not_found", message: "task `t-9` was not found" } } },
    });
    const failure = await serveSource({ fetch, headers: () => ({}) }).task("t-9").catch((error: unknown) => error);
    expect(failure).toMatchObject({ code: "not_found" });
  });
});

describe("serveSource changes", () => {
  const task = (id: string, status: string) => ({ id, title: `Task ${id}`, status });
  const mutation = (action: string, id: string, status: string) => ({ body: { format_version: 1, data: { action, task: task(id, status) } } });
  const auth = () => ({ "X-Arcl-Token": "abc" });
  const posted = { "X-Arcl-Token": "abc", "Content-Type": "application/json" };

  it("posts each task command's options as JSON with the token, escaping the ID", async () => {
    const { fetch, calls } = fakeFetch({
      "/api/tasks": mutation("created", "t-3", "pending"),
      "/api/tasks/t%203/update": mutation("updated", "t 3", "pending"),
      "/api/tasks/t%203/start": mutation("started", "t 3", "in_progress"),
      "/api/tasks/t%203/park": mutation("parked", "t 3", "parked"),
      "/api/tasks/t%203/unpark": mutation("unparked", "t 3", "pending"),
      "/api/tasks/t%203/handoff": mutation("handed_off", "t 3", "parked"),
    });
    const source = serveSource({ fetch, headers: auth });

    expect(await source.createTask({ title: "New", priority: "high" })).toMatchObject({
      value: { id: "t-3", status: "pending" },
      warnings: [],
    });
    await source.updateTask("t 3", { title: "Renamed", body: "Text" });
    expect((await source.startTask("t 3")).value).toMatchObject({ status: "in_progress" });
    await source.startTask("t 3", { force: true });
    await source.parkTask("t 3");
    await source.unparkTask("t 3");
    expect((await source.handOffTask("t 3", "Resume here")).value).toMatchObject({ status: "parked" });

    expect(calls).toEqual([
      { path: "/api/tasks", method: "POST", headers: posted, body: { title: "New", priority: "high" } },
      { path: "/api/tasks/t%203/update", method: "POST", headers: posted, body: { title: "Renamed", body: "Text" } },
      { path: "/api/tasks/t%203/start", method: "POST", headers: posted, body: {} },
      { path: "/api/tasks/t%203/start", method: "POST", headers: posted, body: { force: true } },
      { path: "/api/tasks/t%203/park", method: "POST", headers: posted, body: {} },
      { path: "/api/tasks/t%203/unpark", method: "POST", headers: posted, body: {} },
      { path: "/api/tasks/t%203/handoff", method: "POST", headers: posted, body: { note: "Resume here" } },
    ]);
  });

  it("returns a completion with the tasks it unblocked", async () => {
    const { fetch, calls } = fakeFetch({
      "/api/tasks/t-1/complete": {
        body: { format_version: 2, data: { action: "completed", task: task("t-1", "completed"), completed_subtasks: [], unblocked: ["t-2", "t-4"] } },
      },
    });
    const completion = await serveSource({ fetch, headers: auth }).completeTask("t-1", { evidence: "Delivered by #41." });
    expect(completion).toEqual({
      value: { task: task("t-1", "completed"), completed_subtasks: [], unblocked: ["t-2", "t-4"] },
      warnings: [],
    });
    expect(calls[0].body).toEqual({ evidence: "Delivered by #41." });
  });

  it("returns the warnings a saved change's syncs gave", async () => {
    const warning = { code: "workspace_sync", message: "the change is saved, but the workspace was not updated" };
    const { fetch } = fakeFetch({
      "/api/tasks/t-1/park": { body: { format_version: 1, data: { action: "parked", task: task("t-1", "parked") }, warnings: [warning] } },
    });
    const parked = await serveSource({ fetch, headers: auth }).parkTask("t-1");
    expect(parked).toEqual({ value: task("t-1", "parked"), warnings: [warning] });
  });

  it("raises the CLI's error code and message for a refused change", async () => {
    const { fetch } = fakeFetch({
      "/api/tasks/t-1/start": {
        status: 400,
        body: { format_version: 1, error: { code: "invalid_request", message: "invalid project or record: task `t-1` is blocked", exit_code: 3 } },
      },
    });
    const failure = await serveSource({ fetch, headers: auth }).startTask("t-1").catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(DataError);
    expect(failure).toMatchObject({ code: "invalid_request", message: "invalid project or record: task `t-1` is blocked" });
  });
});

describe("serveSource captures and search", () => {
  it("posts a capture and lists tasks for search", async () => {
    const capture = { id: "c-1", title: "Idea", body: "More", status: "captured", created_at: "2026-10-24T00:00:00Z", project_id: "pj" };
    const { fetch, calls } = fakeFetch({
      "/api/captures": { body: { format_version: 1, data: { action: "created", capture } } },
      "/api/records?kind=task": { body: listing([{ id: "t-1", kind: "task", status: "in_progress", title: "T1: First" }]) },
    });
    const source = serveSource({ fetch, headers: () => ({ "X-Arcl-Token": "abc" }) });

    expect(await source.createCapture({ title: "Idea", body: "More" })).toEqual({ value: capture, warnings: [] });
    expect(await source.taskIndex()).toEqual([{ id: "t-1", title: "T1: First", status: "in_progress" }]);
    expect(calls[0]).toEqual({
      path: "/api/captures",
      method: "POST",
      headers: { "X-Arcl-Token": "abc", "Content-Type": "application/json" },
      body: { title: "Idea", body: "More" },
    });
    expect(calls[1]).toEqual({ path: "/api/records?kind=task", headers: { "X-Arcl-Token": "abc" } });
  });
});

describe("serveSource triage and planning", () => {
  const auth = () => ({ "X-Arcl-Token": "abc" });
  const posted = { "X-Arcl-Token": "abc", "Content-Type": "application/json" };
  const capture = { id: "c 1", title: "Idea !high", body: "More", status: "captured", created_at: "2026-10-24T00:00:00Z", project_id: "pj" };

  it("lists open captures and reads one with its body", async () => {
    const { fetch, calls } = fakeFetch({
      "/api/records?kind=capture&status=captured": { body: listing([{ id: "c 1", kind: "capture", status: "captured", title: "Idea !high" }]) },
      "/api/records/c%201": { body: { format_version: 1, data: { action: "shown", capture } } },
      "/api/records/t-1": { body: { format_version: 2, data: { task: {} } } },
    });
    const source = serveSource({ fetch, headers: auth });
    expect(await source.captures()).toEqual([{ id: "c 1", title: "Idea !high", status: "captured" }]);
    expect(await source.capture("c 1")).toEqual(capture);
    await expect(source.capture("t-1")).rejects.toMatchObject({ code: "not_a_capture" });
    expect(calls.map((call) => call.path)).toEqual(["/api/records?kind=capture&status=captured", "/api/records/c%201", "/api/records/t-1"]);
  });

  it("promotes and discards a capture, naming the record a promotion made", async () => {
    const promoted = (kind: string) => ({
      body: { format_version: 1, data: { capture: { ...capture, status: "promoted" }, promotion: {}, [kind]: { id: `${kind}-1`, title: "Idea" } } },
    });
    const { fetch, calls } = fakeFetch({
      "/api/captures/c%201/promote": promoted("spec"),
      "/api/captures/c%201/discard": { body: { format_version: 1, data: { action: "discarded", capture: { ...capture, status: "discarded" } } } },
    });
    const source = serveSource({ fetch, headers: auth });
    expect(await source.promoteCapture("c 1", { to: "spec", acceptance_criteria: "- works" })).toEqual({
      value: { kind: "spec", id: "spec-1", title: "Idea" },
      warnings: [],
    });
    expect((await source.discardCapture("c 1")).value.status).toBe("discarded");
    expect(calls).toEqual([
      { path: "/api/captures/c%201/promote", method: "POST", headers: posted, body: { to: "spec", acceptance_criteria: "- works" } },
      { path: "/api/captures/c%201/discard", method: "POST", headers: posted, body: {} },
    ]);
  });

  it("reads the tree and the Work list together for Planning, both stopped by one signal", async () => {
    const node = { kind: "spec", id: "s-1", title: "Spec", status: "open", children: [] };
    const { fetch, calls } = fakeFetch({
      "/api/tree": { body: { format_version: 1, data: { nodes: [node] } } },
      "/api/work": { body: { format_version: 2, data: { tasks: [item("t-1", "ph-7")], phases: [phase("ph-7", 7)] } } },
    });
    const signal = new AbortController().signal;
    const planning = await serveSource({ fetch, headers: auth }).planning(signal);
    expect(planning.tree).toEqual([node]);
    expect(planning.work.items[0].milestone?.id).toBe("ph-7");
    expect(calls.map((call) => [call.path, call.signal])).toEqual([
      ["/api/tree", signal],
      ["/api/work", signal],
    ]);
  });

  it("creates a spec and a plan, and applies a plan document", async () => {
    const spec = { id: "s-1", title: "Spec", body: "", acceptance_criteria: "", status: "open" };
    const plan = { id: "pl 1", spec_id: "s-1", title: "Plan", body: "", status: "open" };
    const { fetch, calls } = fakeFetch({
      "/api/specs": { body: { format_version: 1, data: { action: "created", spec } } },
      "/api/plans": { body: { format_version: 1, data: { action: "created", plan } } },
      "/api/plans/pl%201/apply": { body: { format_version: 1, data: { plan, phases: [phase("ph-1", 1)], tasks: [], dependencies: [], diff: {} } } },
    });
    const source = serveSource({ fetch, headers: auth });
    expect((await source.createSpec({ title: "Spec" })).value).toEqual(spec);
    expect((await source.createPlan({ title: "Plan", spec: "s-1" })).value).toEqual(plan);
    expect((await source.applyPlan("pl 1", "format-version = 1")).value).toEqual({
      plan,
      phases: [{ id: "ph-1", plan_id: "pl", title: "Milestone 1", status: "open", position: 1 }],
      tasks: [],
    });
    expect(calls.map((call) => [call.path, call.body])).toEqual([
      ["/api/specs", { title: "Spec" }],
      ["/api/plans", { title: "Plan", spec: "s-1" }],
      ["/api/plans/pl%201/apply", { document: "format-version = 1" }],
    ]);
  });
});

describe("dataSource", () => {
  it("returns the source a build set in place of arcl serve", async () => {
    const refuse = async (id: string) => Promise.reject(new DataError("not_found", id));
    const artifact: DataSource = {
      work: async () => ({ milestones: [], items: [] }),
      task: refuse,
      createTask: async (input) => refuse(input.title),
      updateTask: refuse,
      startTask: refuse,
      parkTask: refuse,
      unparkTask: refuse,
      handOffTask: refuse,
      completeTask: refuse,
      taskIndex: async () => [],
      createCapture: async (input) => refuse(input.title),
      captures: async () => [],
      capture: refuse,
      promoteCapture: refuse,
      discardCapture: refuse,
      planning: async () => ({ tree: [], work: { milestones: [], items: [] } }),
      createSpec: async (input) => refuse(input.title),
      createPlan: async (input) => refuse(input.title),
      applyPlan: refuse,
    };
    setDataSource(artifact);
    expect(dataSource()).toBe(artifact);
    await expect(dataSource().work()).resolves.toEqual({ milestones: [], items: [] });
  });
});
