import { describe, expect, it } from "vitest";
import { planning } from "./fixtures";
import { closedRecords, describeProgress, planningView, planTemplate, visibleRows } from "./view";

describe("planningView", () => {
  it("lists each spec's plans and each plan's milestones in tree order", () => {
    const specs = planningView(planning());
    expect(specs.map((spec) => [spec.id, spec.plans.map((plan) => [plan.id, plan.milestones.map((row) => row.id)])])).toEqual([
      ["s-1", [["pl-1", ["ph-1", "ph-2"]]]],
      ["s-2", [["pl-2", []]]],
      ["s-3", []],
    ]);
  });

  it("counts each milestone's progress, ready and blocked tasks, subtasks included", () => {
    const [first, second] = planningView(planning())[0].plans[0].milestones;
    expect(first).toMatchObject({ title: "phase ph-1", status: "open", done: 1, total: 4, ready: 1, blocked: 1 });
    expect(second).toMatchObject({ done: 1, total: 1, ready: 0, blocked: 0 });
    expect(planningView(planning())[0].plans[0]).toMatchObject({ done: 2, total: 6 });
  });

  it("gives each milestone its task tree", () => {
    const [first] = planningView(planning())[0].plans[0].milestones;
    expect(first.tree.map((task) => [task.id, task.status, task.children?.map((child) => child.id)])).toEqual([
      ["t-1", "completed", []],
      ["t-2", "pending", ["t-3"]],
      ["t-4", "cancelled", []],
      ["t-5", "in_progress", []],
    ]);
    expect(planningView(planning())[0].plans[0].tasks.map((task) => task.id)).toEqual(["t-7"]);
  });
});

describe("visibleRows", () => {
  it("hides the rows under a collapsed spec or plan", () => {
    const specs = planningView(planning());
    const ids = (collapsed: string[]) => visibleRows(specs, new Set(collapsed)).map((row) => row.id);
    expect(ids([])).toEqual(["s-1", "pl-1", "ph-1", "ph-2", "s-2", "pl-2", "s-3"]);
    expect(ids(["pl-1", "s-2"])).toEqual(["s-1", "pl-1", "s-2", "s-3"]);
    expect(ids(["s-1"])).toEqual(["s-1", "s-2", "pl-2", "s-3"]);
  });

  it("starts with closed specs and plans collapsed", () => {
    expect([...closedRecords(planningView(planning()))]).toEqual(["s-2", "pl-2"]);
  });
});

describe("describeProgress", () => {
  it("counts done tasks, or says there are none", () => {
    expect(describeProgress({ done: 2, total: 6 })).toBe("2 of 6 done");
    expect(describeProgress({ done: 0, total: 0 })).toBe("No tasks");
  });
});

describe("planTemplate", () => {
  it("starts at format version 1 with one phase and one task", () => {
    expect(planTemplate()).toMatch(/^format-version = 1\n\n\[\[phases\]\]\n/);
    expect(planTemplate()).toContain("[[phases.tasks]]");
  });
});
