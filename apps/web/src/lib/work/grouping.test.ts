import { describe, expect, it } from "vitest";
import { describeReason, glyphOf, groupWork } from "./grouping";
import { parseFilters } from "./filters";
import { blocked, closed, ready, sampleList, web } from "./fixtures";

const none = parseFilters(new URLSearchParams());

describe("groupWork", () => {
  it("groups by milestone in the CLI's milestone order, with no milestone last", () => {
    const groups = groupWork(sampleList(), none);
    expect(groups.map((group) => group.title)).toEqual([
      "Milestone 5: Agent interface",
      "Milestone 7: Local web app",
      "No milestone",
    ]);
    expect(groups.map((group) => group.detail)).toEqual(["5 tasks", "1 task", "1 task"]);
  });

  it("splits each milestone by status and keeps the CLI's task order inside a status", () => {
    const [agents] = groupWork(sampleList(), none);
    expect(agents.sections.map((section) => [section.title, section.rows.map((row) => row.id)])).toEqual([
      ["In progress", ["t-4"]],
      ["Pending", ["t-2", "t-3", "t-7"]],
      ["Parked", ["t-6"]],
    ]);
  });

  it("puts a phase missing from the milestone list after the known ones", () => {
    const stray = { id: "ph-9", title: "Milestone 9", status: "open" as const };
    const list = { milestones: [web], items: [ready("t-1", stray), ready("t-2", web)] };
    expect(groupWork(list, none).map((group) => group.id)).toEqual(["ph-7", "ph-9"]);
  });

  it("leaves out groups the filters empty", () => {
    const groups = groupWork(sampleList(), parseFilters(new URLSearchParams("status=completed")));
    expect(groups.map((group) => group.id)).toEqual(["ph-7"]);
    expect(groupWork(sampleList(), parseFilters(new URLSearchParams("priority=critical")))).toEqual([]);
  });

  it("names blockers by title in row reasons", () => {
    const [agents] = groupWork(sampleList(), none);
    const row = agents.sections[1].rows.find((candidate) => candidate.id === "t-3");
    expect(row).toMatchObject({ glyph: "blocked", ready: false, reasons: ["blocked by Task t-2 (pending)"] });
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
    const list = { milestones: [web], items: [closed("t-1", "completed", web)] };
    list.items[0].readiness.reasons.push({ kind: "closed_container", message: "phase `ph-7` is `completed`", container: "ph-7" });
    expect(groupWork(list, none)[0].sections[0].rows[0].reasons).toEqual([
      "task status is completed",
      "phase Milestone 7: Local web app is completed",
    ]);
  });

  it("keeps an ID it has no title for", () => {
    const reason = { kind: "closed_container", message: "phase `ph-1` is `completed`", container: "ph-1" };
    expect(describeReason(reason, new Map())).toBe("phase ph-1 is completed");
  });
});
