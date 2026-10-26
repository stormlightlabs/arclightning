import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import type { TaskGroup, TaskRowItem } from "../types";
import TaskListHarness from "./TaskListHarness.svelte";

const row = (id: string, title: string): TaskRowItem => ({
  id,
  title,
  status: "pending",
  priority: "normal",
  glyph: "blocked",
  ready: false,
  reasons: [`${title} is blocked`],
});

const groups: TaskGroup[] = [
  {
    id: "m",
    title: "Milestone",
    sections: [{ id: "m/pending", title: "Pending", rows: [row("t-1", "First task"), row("t-2", "Second task")] }],
  },
];

describe("TaskList", () => {
  it("selects the row that takes focus and shows its readiness reason", async () => {
    await render(TaskListHarness, { groups });
    const first = page.getByRole("link", { name: "First task" });
    await expect.element(page.getByText("First task is blocked")).not.toBeVisible();
    first.element().focus();
    await expect.element(page.getByText("t-1", { exact: true })).toBeInTheDocument();
    await expect.element(page.getByText("First task is blocked")).toBeVisible();
    expect(getComputedStyle(first.element()).outlineStyle).toBe("solid");
  });

  it("shows the readiness card only for pending and parked rows", async () => {
    const closed = (id: string, status: TaskRowItem["status"]): TaskRowItem => ({
      ...row(id, `${status} task`),
      status,
      reasons: [`task status is ${status}`],
    });
    const rows = [
      closed("t-1", "completed"),
      closed("t-2", "in_progress"),
      closed("t-3", "cancelled"),
      closed("t-4", "parked"),
    ];
    await render(TaskListHarness, {
      groups: [{ id: "m", title: "Milestone", sections: [{ id: "m/all", title: "All", rows }] }],
    });
    for (const status of ["completed", "in_progress", "cancelled"]) {
      await expect.element(page.getByText(`task status is ${status}`)).not.toBeInTheDocument();
      expect(
        page
          .getByRole("link", { name: `${status} task` })
          .element()
          .hasAttribute("aria-describedby"),
      ).toBe(false);
    }
    page.getByRole("link", { name: "parked task" }).element().focus();
    await expect.element(page.getByText("task status is parked")).toBeVisible();
  });

  it("reaches only one row with Tab", async () => {
    await render(TaskListHarness, { groups });
    page.getByRole("button", { name: "Select second" }).element().focus();
    await userEvent.keyboard("{Tab}");
    await expect.element(page.getByRole("link", { name: "First task" })).toHaveFocus();
    await userEvent.keyboard("{Tab}");
    await expect.element(page.getByRole("link", { name: "Second task" })).not.toHaveFocus();
  });

  it("moves focus with the selection while focus is in the list", async () => {
    await render(TaskListHarness, { groups });
    page.getByRole("link", { name: "First task" }).element().focus();
    // Select the second row from outside the list without moving focus there.
    (page.getByRole("button", { name: "Select second" }).element() as HTMLButtonElement).click();
    await expect.element(page.getByRole("link", { name: "Second task" })).toHaveFocus();
  });
});
