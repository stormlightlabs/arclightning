import { describe, expect, it } from "vitest";
import { afterRemoval, formFor, promotionOf, TRIAGE_KEYS } from "./triage";

const tasks = [
  { id: "arcl-t-18", title: "T18: Serve the web app", status: "pending" as const },
  { id: "arcl-t-184", title: "T18.4: Add task mutations", status: "completed" as const },
  { id: "arcl-t-x1", title: "T20 first", status: "pending" as const },
  { id: "arcl-t-x2", title: "T20 second", status: "pending" as const },
];

describe("formFor", () => {
  it("reads a task's priority and parent from the capture's tokens", () => {
    expect(formFor("task", "Show unblocked tasks !high ^T18 after:T18.4", tasks)).toEqual({
      to: "task",
      title: "Show unblocked tasks",
      priority: "high",
      phase: "",
      parent: "arcl-t-18",
      notes: ["`after:T18.4` is not set: add the blocker after promoting, with arcl dependency add."],
    });
  });

  it("notes a parent that names no task or several", () => {
    expect(formFor("task", "Idea ^T99", tasks)).toMatchObject({ parent: "", notes: ["`^T99` names no task, so no parent is set."] });
    expect(formFor("task", "Idea ^T20", tasks)).toMatchObject({ parent: "", notes: ["`^T20` names more than one task, so no parent is set."] });
  });

  it("drops the tokens from a spec's or note's title, and keeps a title that is only tokens", () => {
    expect(formFor("spec", "Planned idea !high", tasks)).toEqual({ to: "spec", title: "Planned idea", acceptance: "" });
    expect(formFor("note", "A \\!high thought", tasks)).toEqual({ to: "note", title: "A !high thought" });
    expect(formFor("note", "!high", tasks)).toEqual({ to: "note", title: "!high" });
  });
});

describe("promotionOf", () => {
  it("leaves out blank fields", () => {
    expect(promotionOf({ to: "task", title: "T", priority: "normal", phase: "", parent: "", notes: [] })).toEqual({ to: "task", title: "T", priority: "normal" });
    expect(promotionOf({ to: "task", title: "T", priority: "low", phase: "ph-1", parent: "t-1", notes: ["x"] })).toEqual({
      to: "task",
      title: "T",
      priority: "low",
      phase: "ph-1",
      parent: "t-1",
    });
    expect(promotionOf({ to: "spec", title: "S", acceptance: "  " })).toEqual({ to: "spec", title: "S" });
    expect(promotionOf({ to: "spec", title: "S", acceptance: "- works" })).toEqual({ to: "spec", title: "S", acceptance_criteria: "- works" });
    expect(promotionOf({ to: "note", title: "N" })).toEqual({ to: "note", title: "N" });
  });
});

describe("afterRemoval", () => {
  it("selects the next row, else the one before, else none", () => {
    expect(afterRemoval(["a", "b", "c"], "b")).toBe("c");
    expect(afterRemoval(["a", "b", "c"], "c")).toBe("b");
    expect(afterRemoval(["a"], "a")).toBeNull();
    expect(afterRemoval(["a", "b"], "z")).toBe("a");
  });
});

describe("TRIAGE_KEYS", () => {
  it("gives each action its own key", () => {
    const keys = Object.values(TRIAGE_KEYS);
    expect(new Set(keys).size).toBe(keys.length);
  });
});
