import { describe, expect, it } from "vitest";
import { DataError, type DataSource } from "$lib/data";
import { task } from "$lib/work/fixtures";
import { actionsFor, draftOf, editOf, failureOf, runAction, unblockedTasks } from "./actions";
import { blockedDetail } from "./fixtures";

/** A data source that records each change and answers with `task("t-1")`. */
function recorder() {
  const calls: unknown[][] = [];
  const answer = (name: string) => async (...args: unknown[]) => (calls.push([name, ...args]), task("t-1"));
  const source = {
    work: async () => ({ milestones: [], items: [] }),
    task: async () => blockedDetail(),
    createTask: answer("createTask"),
    updateTask: answer("updateTask"),
    startTask: answer("startTask"),
    parkTask: answer("parkTask"),
    unparkTask: answer("unparkTask"),
    handOffTask: answer("handOffTask"),
    completeTask: async (...args: unknown[]) => (
      calls.push(["completeTask", ...args]), { task: task("t-1"), completed_subtasks: [], unblocked: ["t-10.5"] }
    ),
  } as DataSource;
  return { source, calls };
}

describe("actionsFor", () => {
  it("offers the lifecycle changes each status allows, the usual next step first", () => {
    expect(actionsFor("pending")).toEqual(["start", "park"]);
    expect(actionsFor("in_progress")).toEqual(["complete", "handoff", "park"]);
    expect(actionsFor("parked")).toEqual(["unpark"]);
    expect(actionsFor("completed")).toEqual([]);
    expect(actionsFor("cancelled")).toEqual([]);
  });
});

describe("runAction", () => {
  it("runs each action through the data source with its text", async () => {
    const { source, calls } = recorder();
    await runAction(source, "t-1", "start");
    await runAction(source, "t-1", "park");
    await runAction(source, "t-1", "unpark");
    await runAction(source, "t-1", "handoff", "Resume at the tests");
    expect(await runAction(source, "t-1", "complete", "Delivered by #41.")).toEqual({ unblocked: ["t-10.5"] });
    expect(await runAction(source, "t-1", "complete", "  \n")).toEqual({ unblocked: ["t-10.5"] });
    expect(calls).toEqual([
      ["startTask", "t-1"],
      ["parkTask", "t-1"],
      ["unparkTask", "t-1"],
      ["handOffTask", "t-1", "Resume at the tests"],
      ["completeTask", "t-1", { evidence: "Delivered by #41." }],
      ["completeTask", "t-1", {}],
    ]);
  });

  it("passes the data source's failure through", async () => {
    const { source } = recorder();
    source.startTask = async () => Promise.reject(new DataError("invalid_request", "task `t-1` is blocked"));
    await expect(runAction(source, "t-1", "start")).rejects.toMatchObject({ code: "invalid_request" });
  });
});

describe("editOf", () => {
  it("sends only the fields that changed, as typed", () => {
    const original = task("t-1", { title: "Title", body: "Body", priority: "normal" });
    expect(editOf(original, draftOf(original))).toEqual({});
    expect(editOf(original, { ...draftOf(original), title: "  " })).toEqual({ title: "  " });
    expect(editOf(original, { title: "Title", body: "New", priority: "high" })).toEqual({ body: "New", priority: "high" });
  });
});

describe("failureOf", () => {
  it("keeps the CLI's code and message", () => {
    expect(failureOf(new DataError("conflict", "changed since read"))).toEqual({ code: "conflict", message: "changed since read" });
    expect(failureOf(new TypeError("Failed to fetch"))).toEqual({ code: "unavailable", message: "Failed to fetch" });
  });
});

describe("unblockedTasks", () => {
  it("titles each unblocked task from the reloaded dependents", () => {
    expect(unblockedTasks(["t-10.5", "t-99"], blockedDetail())).toEqual([
      { id: "t-10.5", title: "T10.5: Document the MCP tools" },
      { id: "t-99", title: "t-99" },
    ]);
  });
});
