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
  const calls: { path: string; headers: Record<string, string>; signal?: AbortSignal | null }[] = [];
  const fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const path = String(input);
    calls.push({ path, headers: (init?.headers ?? {}) as Record<string, string>, signal: init?.signal });
    const route = routes[path];
    if (!route) return new Response("{}", { status: 404 });
    return new Response(JSON.stringify(route.body), { status: route.status ?? 200 });
  }) as typeof globalThis.fetch;
  return { fetch, calls };
}

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

describe("dataSource", () => {
  it("returns the source a build set in place of arcl serve", async () => {
    const artifact: DataSource = { work: async () => ({ milestones: [], items: [] }) };
    setDataSource(artifact);
    expect(dataSource()).toBe(artifact);
    await expect(dataSource().work()).resolves.toEqual({ milestones: [], items: [] });
  });
});
