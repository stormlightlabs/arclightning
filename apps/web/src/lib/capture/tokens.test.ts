import { describe, expect, it } from "vitest";
import { captureOf, describeToken, parseCapture, resolveRef, taskOf } from "./tokens";

const tasks = [
  { id: "arcl-t-1", title: "T18: Serve the local web app", status: "pending" as const },
  { id: "arcl-t-2", title: "T18.4: Add task mutations", status: "in_progress" as const },
  { id: "arcl-t-3", title: "T18.4 duplicate key", status: "pending" as const },
  { id: "arcl-t-4", title: "T18.3 Build the Task view", status: "completed" as const },
];

describe("parseCapture", () => {
  it("takes the first line as the title and the rest as the body", () => {
    expect(parseCapture("  Fix the menu  \n\nIt opens twice.\n")).toEqual({ title: "Fix the menu", body: "It opens twice.", line: "Fix the menu", tokens: [] });
  });

  it("pulls priority, parent and blocker tokens out of the title", () => {
    const parsed = parseCapture("Show unblocked tasks !HIGH ^T18 after:T18.4 after:arcl-t-4");
    expect(parsed.title).toBe("Show unblocked tasks");
    expect(parsed.line).toBe("Show unblocked tasks !HIGH ^T18 after:T18.4 after:arcl-t-4");
    expect(parsed.tokens).toEqual([
      { kind: "priority", text: "!HIGH", priority: "high" },
      { kind: "parent", text: "^T18", ref: "T18" },
      { kind: "blocker", text: "after:T18.4", ref: "T18.4" },
      { kind: "blocker", text: "after:arcl-t-4", ref: "arcl-t-4" },
    ]);
    expect(parsed.tokens.map(describeToken)).toEqual(["high priority", "subtask of T18", "blocked by T18.4", "blocked by arcl-t-4"]);
  });

  it("leaves words that only look like tokens, and the body, alone", () => {
    const parsed = parseCapture("Ship it !later ^ after: wow!\n!high ^T18 after:T1");
    expect(parsed.title).toBe("Ship it !later ^ after: wow!");
    expect(parsed.tokens).toEqual([]);
    expect(parsed.body).toBe("!high ^T18 after:T1");
  });
});

describe("captureOf", () => {
  it("files the first line as typed, tokens included, with the body", () => {
    expect(captureOf(parseCapture("Idea !high"))).toEqual({ title: "Idea !high" });
    expect(captureOf(parseCapture("Idea\nMore"))).toEqual({ title: "Idea", body: "More" });
  });
});

describe("taskOf", () => {
  it("sets the task's fields from the tokens, resolving title keys and IDs", () => {
    const result = taskOf(parseCapture("Show unblocked !low !critical ^T18 after:t18.4 after:arcl-t-4 after:arcl-t-4\nBody"), [tasks[0], tasks[1], tasks[3]]);
    expect(result).toEqual({
      task: { title: "Show unblocked", body: "Body", priority: "critical", parent: "arcl-t-1", blocked_by: ["arcl-t-2", "arcl-t-4"] },
    });
  });

  it("names references that match no task or several", () => {
    expect(taskOf(parseCapture("X ^T99 after:T18.4"), tasks)).toEqual({
      problems: [
        { ref: "T99", problem: "missing" },
        { ref: "T18.4", problem: "ambiguous" },
      ],
    });
  });

  it("files plain text as a task with only a title", () => {
    expect(taskOf(parseCapture("Just a title"), [])).toEqual({ task: { title: "Just a title" } });
  });
});

describe("resolveRef", () => {
  it("matches an ID exactly, or a title key followed by a colon or a space", () => {
    expect(resolveRef("arcl-t-2", tasks)).toBe("arcl-t-2");
    expect(resolveRef("t18", tasks)).toBe("arcl-t-1");
    expect(resolveRef("T18.3", tasks)).toBe("arcl-t-4");
    expect(resolveRef("T18.", tasks)).toEqual({ ref: "T18.", problem: "missing" });
  });
});
