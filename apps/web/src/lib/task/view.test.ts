import { describe, expect, it } from "vitest";
import { blockedDetail, bareDetail, doneBlockersDetail } from "./fixtures";
import { blockersOf, inheritedGroups, propertiesOf, reasonSegments, taskKeyAction, titlesOf } from "./view";

const key = (key: string, fields: Partial<Parameters<typeof taskKeyAction>[0]> = {}) => ({
  key,
  altKey: false,
  ctrlKey: false,
  metaKey: false,
  defaultPrevented: false,
  target: null,
  ...fields,
});

describe("reasonSegments", () => {
  const detail = blockedDetail();
  const titles = titlesOf(detail);
  const [direct, inherited, container] = detail.readiness.reasons;

  it("links a direct blocker to its Task view by title", () => {
    expect(reasonSegments(direct, titles)).toEqual([
      { text: "blocked by " },
      { id: "t-10.3", label: "T10.3: Add the MCP server", href: "/tasks/t-10.3" },
      { text: " (in_progress)" },
    ]);
  });

  it("links both the ancestor and the blocker of an inherited blocker", () => {
    expect(reasonSegments(inherited, titles)).toEqual([
      { text: "ancestor " },
      { id: "t-10", label: "T10: Add the MCP server", href: "/tasks/t-10" },
      { text: " is blocked by " },
      { id: "t-17", label: "T17: Report readiness as data", href: "/tasks/t-17" },
      { text: " (pending)" },
    ]);
  });

  it("names a container by title and leaves a quoted status as written", () => {
    expect(reasonSegments(container, titles)).toEqual([
      { text: "phase " },
      { text: "Milestone 5: Agent interface" },
      { text: " is " },
      { text: "completed" },
    ]);
  });

  it("links a task it has no title for by ID", () => {
    const missing = { kind: "missing_blocker", message: "blocked by missing task `t-404`", blocker: "t-404" };
    expect(reasonSegments(missing, new Map())).toContainEqual({ id: "t-404", label: "t-404", href: "/tasks/t-404" });
  });
});

describe("inheritedGroups", () => {
  it("groups inherited blockers under the ancestor that holds them", () => {
    const groups = inheritedGroups(blockedDetail());
    expect(groups.map((group) => [group.ancestor, group.blockers.map((blocker) => blocker.task.id)])).toEqual([
      [{ id: "t-10", title: "T10: Add the MCP server" }, ["t-17"]],
    ]);
  });

  it("is empty without inherited blockers", () => {
    expect(inheritedGroups(bareDetail())).toEqual([]);
  });
});

describe("blockersOf", () => {
  it("keeps completed blockers out of the ones the task waits on", () => {
    const { direct, inherited, done } = blockersOf(doneBlockersDetail());
    expect(direct.map((blocker) => blocker.task.id)).toEqual(["t-10.3"]);
    expect(inherited.map((group) => group.blockers.map((blocker) => blocker.task.id))).toEqual([["t-17"]]);
    expect(done.map(({ blocker, ancestor }) => [blocker.task.id, blocker.evidence, ancestor])).toEqual([
      ["t-10.2", "Landed in **#38**.", null],
      ["t-9", "", { id: "t-10", title: "T10: Add the MCP server" }],
    ]);
  });

  it("leaves out an ancestor whose blockers are all completed", () => {
    const detail = doneBlockersDetail();
    detail.inherited_blockers = detail.inherited_blockers.filter((blocker) => blocker.completed);
    expect(blockersOf(detail).inherited).toEqual([]);
  });
});

describe("propertiesOf", () => {
  it("lists the ID, status, readiness, priority, parent, spec, plan and phase", () => {
    const detail = blockedDetail();
    expect(propertiesOf(detail)).toEqual([
      { label: "ID", value: detail.task.id, copy: true },
      { label: "Status", value: "Pending", status: "pending" },
      { label: "Readiness", value: "Blocked" },
      { label: "Priority", value: "normal", priority: "normal" },
      { label: "Parent", value: "T10: Add the MCP server", href: "/tasks/t-10" },
      { label: "Spec", value: "First release" },
      { label: "Plan", value: "Roadmap" },
      { label: "Phase", value: "Milestone 5: Agent interface" },
    ]);
  });

  it("leaves absent records empty", () => {
    const values = Object.fromEntries(propertiesOf(bareDetail()).map((property) => [property.label, property.value]));
    expect(values).toMatchObject({ Readiness: "Ready", Parent: null, Spec: null, Plan: null, Phase: null });
  });
});

describe("taskKeyAction", () => {
  it("returns to the Work list on Escape and toggles the panel on ]", () => {
    expect(taskKeyAction(key("Escape"))).toBe("back");
    expect(taskKeyAction(key("]"))).toBe("properties");
    expect(taskKeyAction(key("j"))).toBeNull();
  });

  it("stays on the task on Escape while a form holds unsaved input", () => {
    expect(taskKeyAction(key("Escape"), true)).toBeNull();
    expect(taskKeyAction(key("]"), true)).toBe("properties");
    expect(taskKeyAction(key("Escape"), false)).toBe("back");
  });

  it("toggles the panel on ] typed with AltGr or Option", () => {
    const altGraph = (on: boolean) => (name: string) => name === "AltGraph" && on;
    expect(taskKeyAction(key("]", { ctrlKey: true, altKey: true, getModifierState: altGraph(true) }))).toBe(
      "properties",
    );
    expect(taskKeyAction(key("]", { ctrlKey: true, altKey: true }))).toBe("properties");
    expect(taskKeyAction(key("]", { altKey: true, getModifierState: altGraph(false) }))).toBe("properties");
    expect(taskKeyAction(key("]", { ctrlKey: true, altKey: true, getModifierState: altGraph(false) }))).toBeNull();
  });

  it("leaves keys that are handled, held with a modifier, or typed into a field", () => {
    expect(taskKeyAction(key("Escape", { defaultPrevented: true }))).toBeNull();
    expect(taskKeyAction(key("]", { metaKey: true }))).toBeNull();
    expect(taskKeyAction(key("]", { ctrlKey: true }))).toBeNull();
    expect(taskKeyAction(key("Escape", { altKey: true }))).toBeNull();
    const editor = {
      closest: (selector: string) => (selector.includes("contenteditable") ? {} : null),
    } as unknown as EventTarget;
    expect(taskKeyAction(key("Escape", { target: editor }))).toBeNull();
  });
});
