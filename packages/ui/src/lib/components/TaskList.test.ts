import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import type { TaskGroup, TaskRowItem } from "../types";
import TaskGlyph from "./TaskGlyph.svelte";
import TaskList, { rowOrder } from "./TaskList.svelte";
import TaskRow from "./TaskRow.svelte";

const blocked: TaskRowItem = {
  id: "t-2",
  title: "Install the skill",
  status: "pending",
  priority: "high",
  glyph: "blocked",
  ready: false,
  reasons: ["blocked by Add the skill (pending)"],
};

const ready: TaskRowItem = { ...blocked, id: "t-1", title: "Add the skill", priority: "normal", glyph: "ready", ready: true, reasons: [] };

const groups: TaskGroup[] = [
  { id: "m-5", title: "Milestone 5", detail: "2 tasks", sections: [{ id: "m-5/pending", title: "Pending", rows: [ready, blocked] }] },
  { id: "none", title: "No milestone", sections: [{ id: "none/done", title: "Completed", rows: [{ ...ready, id: "t-3", status: "completed", glyph: "done" }] }] },
];

describe("TaskRow", () => {
  it("links the title, labels the glyph, and describes the row with its readiness reasons", () => {
    const { body } = render(TaskRow, { props: { row: blocked, href: "/tasks/t-2" } });
    expect(body).toContain('href="/tasks/t-2"');
    expect(body).toContain('aria-label="Blocked"');
    expect(body).toContain('role="tooltip"');
    expect(body).toContain("blocked by Add the skill (pending)");
    const describedBy = /aria-describedby="([^"]+)"/.exec(body)?.[1];
    expect(describedBy).toBeTruthy();
    expect(body).toContain(`id="${describedBy}"`);
    expect(body).toContain(">High<");
  });

  it("leaves the priority out at normal", () => {
    const { body } = render(TaskRow, { props: { row: ready, href: "/tasks/t-1" } });
    expect(body).not.toContain(">Normal<");
    expect(body).toContain("Ready to start");
  });
});

describe("TaskGlyph", () => {
  it("marks each kind for styling and hides an unlabelled glyph", () => {
    const { body } = render(TaskGlyph, { props: { kind: "in_progress" } });
    expect(body).toContain('data-kind="in_progress"');
    expect(body).toContain('aria-hidden="true"');
  });
});

describe("TaskList", () => {
  it("renders groups, then sections, then rows in the given order", () => {
    const { body } = render(TaskList, { props: { groups, href: (id: string) => `/tasks/${id}` } });
    const at = (text: string) => body.indexOf(text);
    expect(at("Milestone 5")).toBeLessThan(at("Add the skill"));
    expect(at("Add the skill")).toBeLessThan(at("Install the skill"));
    expect(at("Install the skill")).toBeLessThan(at("No milestone"));
    expect(body.match(/tabindex="0"/g)).toHaveLength(1);
  });

  it("makes the selected row the one Tab reaches", () => {
    const { body } = render(TaskList, { props: { groups, href: (id: string) => `/tasks/${id}`, selectedId: "t-2" } });
    const tabbable = /<a[^>]*tabindex="0"[^>]*>/.exec(body)?.[0] ?? "";
    expect(tabbable).toContain('href="/tasks/t-2"');
    expect(tabbable).toContain('aria-current="true"');
  });

  it("lists row IDs in display order", () => {
    expect(rowOrder(groups)).toEqual(["t-1", "t-2", "t-3"]);
  });
});
