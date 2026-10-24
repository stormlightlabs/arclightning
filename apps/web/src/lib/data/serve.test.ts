import { describe, expect, it } from "vitest";
import { DataError, dataSource, serveSource, setDataSource, type DataSource } from "./index";

const phase = { id: "ph-7", project_id: "pj", plan_id: "pl", title: "Milestone 7", body: "", status: "open", position: 7 };

function explanation(id: string, phaseId: string | null) {
  return {
    format_version: 2,
    data: {
      task: { id, title: `Task ${id}`, status: "pending", priority: "normal", phase_id: null },
      readiness: { ready: true, blocked: false, reasons: [] },
      phase: phaseId === null ? null : phase,
      blockers: [],
    },
  };
}

/** A fetch that answers from `routes` and records each request. */
function fakeFetch(routes: Record<string, { status?: number; body: unknown }>) {
  const calls: { path: string; headers: Record<string, string>; method?: string; body?: unknown }[] = [];
  const fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input);
    const call: (typeof calls)[number] = { path, headers: (init?.headers ?? {}) as Record<string, string> };
    if (init?.method) call.method = init.method;
    if (init?.body !== undefined) call.body = JSON.parse(String(init.body));
    calls.push(call);
    const route = routes[path];
    if (!route) return new Response("{}", { status: 404 });
    return new Response(JSON.stringify(route.body), { status: route.status ?? 200 });
  }) as typeof globalThis.fetch;
  return { fetch, calls };
}

const listing = (records: { id: string; kind: string; status: string; title: string }[]) => ({ format_version: 1, data: { records } });

describe("serveSource", () => {
  it("builds the Work list from list, explain and show, with the token on every request", async () => {
    const { fetch, calls } = fakeFetch({
      "/api/records?kind=task": {
        body: listing([
          { id: "t-2", kind: "task", status: "pending", title: "Task t-2" },
          { id: "t-1", kind: "task", status: "pending", title: "Task t-1" },
        ]),
      },
      "/api/records?kind=phase": {
        body: listing([
          { id: "ph-7", kind: "phase", status: "open", title: "Milestone 7" },
          { id: "ph-8", kind: "phase", status: "open", title: "Milestone 8" },
        ]),
      },
      "/api/tasks/t-2/explain": { body: explanation("t-2", "ph-7") },
      "/api/tasks/t-1/explain": { body: explanation("t-1", null) },
      "/api/records/ph-8": { body: { format_version: 1, data: { action: "shown", phase: { ...phase, id: "ph-8", title: "Milestone 8", position: 8 } } } },
    });

    const work = await serveSource({ fetch, headers: () => ({ "X-Arcl-Token": "abc" }) }).work();

    expect(work.milestones).toEqual([
      { id: "ph-7", plan_id: "pl", title: "Milestone 7", status: "open", position: 7 },
      { id: "ph-8", plan_id: "pl", title: "Milestone 8", status: "open", position: 8 },
    ]);
    expect(work.items.map((item) => [item.task.id, item.milestone?.id ?? null])).toEqual([
      ["t-2", "ph-7"],
      ["t-1", null],
    ]);
    expect(work.items[0].milestone).toEqual({ id: "ph-7", plan_id: "pl", title: "Milestone 7", status: "open", position: 7 });
    expect(calls).toHaveLength(5);
    expect(calls.every((call) => call.headers["X-Arcl-Token"] === "abc")).toBe(true);
  });

  it("raises the CLI's error code and message", async () => {
    const { fetch } = fakeFetch({
      "/api/records?kind=task": { status: 401, body: { format_version: 1, error: { code: "unauthorized", message: "the X-Arcl-Token header is missing" } } },
      "/api/records?kind=phase": { body: listing([]) },
    });
    const failure = await serveSource({ fetch, headers: () => ({}) }).work().catch((error: unknown) => error);
    expect(failure).toBeInstanceOf(DataError);
    expect(failure).toMatchObject({ code: "unauthorized", message: "the X-Arcl-Token header is missing" });
  });

  it("leaves out a task deleted between the list and its explain", async () => {
    const gone = { format_version: 1, error: { code: "not_found", message: "planning task `t-1` was not found" } };
    const { fetch } = fakeFetch({
      "/api/records?kind=task": {
        body: listing([
          { id: "t-1", kind: "task", status: "pending", title: "Task t-1" },
          { id: "t-2", kind: "task", status: "pending", title: "Task t-2" },
        ]),
      },
      "/api/records?kind=phase": { body: listing([]) },
      "/api/tasks/t-1/explain": { status: 404, body: gone },
      "/api/tasks/t-2/explain": { body: explanation("t-2", null) },
    });
    const work = await serveSource({ fetch, headers: () => ({}) }).work();
    expect(work.items.map((item) => item.task.id)).toEqual(["t-2"]);
  });

  it("fails the list on an explain error other than not_found", async () => {
    const { fetch } = fakeFetch({
      "/api/records?kind=task": { body: listing([{ id: "t-1", kind: "task", status: "pending", title: "Task t-1" }]) },
      "/api/records?kind=phase": { body: listing([]) },
      "/api/tasks/t-1/explain": { status: 500, body: { format_version: 1, error: { code: "storage", message: "database is locked" } } },
    });
    const failure = await serveSource({ fetch, headers: () => ({}) }).work().catch((error: unknown) => error);
    expect(failure).toMatchObject({ code: "storage", message: "database is locked" });
  });
});

describe("serveSource.task", () => {
  it("reads the task's show envelope, escaping the ID, with the token", async () => {
    const detail = {
      ...explanation("t 1", null).data,
      blockers: [],
      inherited_blockers: [],
      dependents: [],
      children: [],
      ancestors: [],
      spec: null,
      plan: null,
    };
    const { fetch, calls } = fakeFetch({ "/api/records/t%201": { body: { format_version: 2, data: detail } } });
    const loaded = await serveSource({ fetch, headers: () => ({ "X-Arcl-Token": "abc" }) }).task("t 1");
    expect(loaded).toEqual(detail);
    expect(calls).toEqual([{ path: "/api/records/t%201", headers: { "X-Arcl-Token": "abc" } }]);
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

    expect(await source.createTask({ title: "New", priority: "high" })).toMatchObject({ id: "t-3", status: "pending" });
    await source.updateTask("t 3", { title: "Renamed", body: "Text" });
    expect(await source.startTask("t 3")).toMatchObject({ status: "in_progress" });
    await source.startTask("t 3", { force: true });
    await source.parkTask("t 3");
    await source.unparkTask("t 3");
    expect(await source.handOffTask("t 3", "Resume here")).toMatchObject({ status: "parked" });

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
    expect(completion).toEqual({ task: task("t-1", "completed"), completed_subtasks: [], unblocked: ["t-2", "t-4"] });
    expect(calls[0].body).toEqual({ evidence: "Delivered by #41." });
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
    };
    setDataSource(artifact);
    expect(dataSource()).toBe(artifact);
    await expect(dataSource().work()).resolves.toEqual({ milestones: [], items: [] });
  });
});
