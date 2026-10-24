import { describe, expect, it } from "vitest";
import type { Milestone } from "$lib/data";
import { describeReason, glyphOf, groupWork, orderMilestones } from "./grouping";
import { parseFilters } from "./filters";
import { agents, blocked, closed, ready, sampleList, web } from "./fixtures";

const none = parseFilters(new URLSearchParams());

describe("groupWork", () => {
  it("groups by milestone in plan and position order, with no milestone last", () => {
    const groups = groupWork(sampleList(), none);
    expect(groups.map((group) => group.title)).toEqual([
      "Milestone 5: Agent interface",
      "Milestone 7: Local web app",
      "No milestone",
    ]);
    expect(groups.map((group) => group.detail)).toEqual(["6 tasks", "1 task", "1 task"]);
  });

  it("splits each milestone by status and orders tasks by position, each followed by its subtasks", () => {
    const [agents] = groupWork(sampleList(), none);
    expect(agents.sections.map((section) => [section.title, section.rows.map((row) => row.id)])).toEqual([
      ["In progress", ["t-4"]],
      ["Pending", ["t-3", "t-8", "t-7", "t-2"]],
      ["Parked", ["t-6"]],
    ]);
  });

  it("orders milestones by plan before position", () => {
    const later: Milestone = { id: "ph-1", plan_id: "pl-2", title: "Later plan", status: "open", position: 1 };
    const list = { milestones: [later, web, agents], items: [ready("t-1", later), ready("t-2", web), ready("t-3", agents)] };
    expect(groupWork(list, none).map((group) => group.id)).toEqual(["ph-5", "ph-10", "ph-1"]);
  });

  it("keeps a subtask with its parent's position, not its own", () => {
    const list = {
      milestones: [agents],
      items: [
        ready("t-1", agents, { position: 1 }),
        ready("t-2", agents, { position: 0 }),
        ready("t-3", agents, { parent_id: "t-1", position: 0 }),
        ready("t-4", agents, { parent_id: "gone", position: 2 }),
      ],
    };
    expect(groupWork(list, none)[0].sections[0].rows.map((row) => row.id)).toEqual(["t-2", "t-1", "t-3", "t-4"]);
  });

  it("places a phase missing from the milestone list by its plan and position", () => {
    const stray: Milestone = { id: "ph-9", plan_id: "pl-1", title: "Milestone 6", status: "open", position: 6 };
    const list = { milestones: [web, agents], items: [ready("t-1", stray), ready("t-2", web), ready("t-3", agents)] };
    expect(groupWork(list, none).map((group) => group.id)).toEqual(["ph-5", "ph-9", "ph-10"]);
  });

  it("leaves out groups the filters empty", () => {
    const groups = groupWork(sampleList(), parseFilters(new URLSearchParams("status=completed")));
    expect(groups.map((group) => group.id)).toEqual(["ph-10"]);
    expect(groupWork(sampleList(), parseFilters(new URLSearchParams("priority=critical")))).toEqual([]);
  });

  it("names blockers by title in row reasons", () => {
    const [agents] = groupWork(sampleList(), none);
    const row = agents.sections[1].rows.find((candidate) => candidate.id === "t-3");
    expect(row).toMatchObject({ glyph: "blocked", ready: false, reasons: ["blocked by Task t-2 (pending)"] });
  });
});

describe("orderMilestones", () => {
  it("lists milestones for the filter in the same order as the groups", () => {
    expect(orderMilestones(sampleList()).map((milestone) => milestone.id)).toEqual(["ph-5", "ph-10"]);
  });
});

describe("glyphOf", () => {
  it("shows status once work started or closed, and readiness before", () => {
    expect(glyphOf(ready("a", null))).toBe("ready");
    expect(glyphOf(blocked("a", "b", null))).toBe("blocked");
    expect(glyphOf(closed("a", "parked", null))).toBe("waiting");
    expect(glyphOf(closed("a", "in_progress", null))).toBe("in_progress");
    expect(glyphOf(closed("a", "completed", null))).toBe("done");
    expect(glyphOf(closed("a", "cancelled", null))).toBe("cancelled");
  });
});

describe("describeReason", () => {
  it("names a milestone by title", () => {
    const list = { milestones: [web], items: [closed("t-1", "parked", web)] };
    list.items[0].readiness.reasons.push({ kind: "closed_container", message: "phase `ph-10` is `completed`", container: "ph-10" });
    expect(groupWork(list, none)[0].sections[0].rows[0].reasons).toEqual([
      "task status is parked",
      "phase Milestone 7: Local web app is completed",
    ]);
  });

  it("keeps an ID it has no title for", () => {
    const reason = { kind: "closed_container", message: "phase `ph-1` is `completed`", container: "ph-1" };
    expect(describeReason(reason, new Map())).toBe("phase ph-1 is completed");
  });
});
