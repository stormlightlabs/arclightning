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
  const calls: { path: string; headers: Record<string, string> }[] = [];
  const fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input);
    calls.push({ path, headers: (init?.headers ?? {}) as Record<string, string> });
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

describe("dataSource", () => {
  it("returns the source a build set in place of arcl serve", async () => {
    const artifact: DataSource = {
      work: async () => ({ milestones: [], items: [] }),
      task: async (id) => Promise.reject(new DataError("not_found", id)),
    };
    setDataSource(artifact);
    expect(dataSource()).toBe(artifact);
    await expect(dataSource().work()).resolves.toEqual({ milestones: [], items: [] });
  });
});
