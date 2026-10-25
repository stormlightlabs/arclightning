import { page as screen, userEvent } from "vitest/browser";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import "@arclightning/ui/styles.css";
import { registeredCommands } from "$lib/commands";
import { DataError, QUEUED, setDataSource, type DataSource, type WorkList } from "$lib/data";
import { VIEW_STORAGE_KEY } from "$lib/work";
import { sampleList, task } from "$lib/work/fixtures";
import WorkRoute from "../../routes/+page.svelte";
import { goto, reset, show } from "./app.svelte";
import { fakeSource } from "./source";

vi.mock("$app/state", async () => ({ page: (await import("./app.svelte")).page }));
vi.mock("$app/navigation", async () => {
  const app = await import("./app.svelte");
  return { afterNavigate: app.afterNavigate, goto: app.goto };
});

/** A source whose Work list starts as `list`; each lifecycle change applies to it and answers. */
function source(list: WorkList = sampleList(), answers: Partial<DataSource> = {}) {
  const changed = (status: "in_progress" | "parked" | "pending") => async (id: string) => {
    const item = list.items.find((entry) => entry.task.id === id)!;
    item.task = { ...item.task, status };
    return { value: item.task, warnings: [] };
  };
  const made = fakeSource({
    work: async () => structuredClone(list),
    startTask: changed("in_progress"),
    parkTask: changed("parked"),
    unparkTask: changed("pending"),
    completeTask: async (id) => {
      const item = list.items.find((entry) => entry.task.id === id)!;
      item.task = { ...item.task, status: "completed" };
      return { value: { task: item.task, completed_subtasks: [], unblocked: [] }, warnings: [] };
    },
    ...answers,
  });
  setDataSource(made.source);
  return made;
}

/** The column for `title`, found by its heading. */
const column = (title: string) => screen.getByRole("region", { name: "Board" }).getByRole("list", { name: new RegExp(`^${title}`) });

beforeEach(() => {
  reset();
  window.localStorage.removeItem(VIEW_STORAGE_KEY);
});

describe("Work board", () => {
  it("shows a column per status with cards that link to the Task view", async () => {
    source();
    show("/?view=board");
    await render(WorkRoute);
    const pending = column("Pending");
    await expect.element(pending.getByRole("listitem")).toHaveLength(5);
    const card = pending.getByRole("listitem").filter({ hasText: "Task t-2" });
    await expect.element(card).toHaveTextContent("t-2");
    await expect.element(card).toHaveTextContent("High");
    await expect.element(card).toHaveTextContent("Ready");
    await expect.element(card.getByRole("link", { name: "Task t-2" })).toHaveAttribute("href", "/tasks/t-2");
    await expect.element(pending.getByRole("listitem").filter({ hasText: "Task t-3" })).toHaveTextContent("Blocked");
    await expect.element(column("Cancelled").getByRole("listitem")).toHaveLength(0);
    await expect.element(screen.getByRole("button", { name: "Board" })).toHaveAttribute("aria-pressed", "true");
  });

  it("follows the filters in the URL", async () => {
    source();
    show("/?view=board&readiness=ready&milestone=ph-5");
    await render(WorkRoute);
    await expect.element(column("Pending").getByRole("listitem")).toHaveLength(3);
    await expect.element(column("In progress").getByRole("listitem")).toHaveLength(0);
    await expect.element(column("Pending")).not.toHaveTextContent("Task t-5");
  });

  it("remembers the layout chosen with the toggle and names it in the URL", async () => {
    source();
    show("/?status=pending");
    await render(WorkRoute);
    await expect.element(screen.getByRole("region", { name: "Work" })).toBeVisible();
    await screen.getByRole("button", { name: "Board" }).click();
    expect(window.localStorage.getItem(VIEW_STORAGE_KEY)).toBe("board");
    expect(goto).toHaveBeenLastCalledWith("/?status=pending&view=board", expect.objectContaining({ replaceState: true }));
  });

  it("moves a card with the keyboard through its Move dialog", async () => {
    const { calls } = source();
    show("/?view=board");
    await render(WorkRoute);
    const move = screen.getByRole("button", { name: "Move Task t-7" });
    await expect.element(move).toBeVisible();
    (move.element() as HTMLElement).focus();
    await userEvent.keyboard("{Enter}");
    const dialog = screen.getByRole("dialog", { name: "Move Task t-7" });
    await expect.element(dialog).toBeVisible();
    await expect.element(dialog.getByRole("button")).toHaveLength(4);
    await dialog.getByRole("button", { name: "In progress" }).click();
    await expect.element(column("In progress").getByRole("listitem").filter({ hasText: "Task t-7" })).toBeVisible();
    expect(calls).toContainEqual(["startTask", "t-7"]);
  });

  it("asks for evidence before a completion", async () => {
    const { calls } = source();
    show("/?view=board");
    await render(WorkRoute);
    await screen.getByRole("button", { name: "Move Task t-4" }).click();
    await screen.getByRole("dialog").getByRole("button", { name: "Completed" }).click();
    const dialog = screen.getByRole("dialog", { name: "Complete Task t-4" });
    await expect.element(dialog.getByRole("textbox", { name: "Evidence Markdown" })).toHaveFocus();
    await userEvent.keyboard("Shipped.");
    await dialog.getByRole("button", { name: "Complete", exact: true }).click();
    await expect.element(column("Completed").getByRole("listitem").filter({ hasText: "Task t-4" })).toBeVisible();
    expect(calls).toContainEqual(["completeTask", "t-4", { evidence: "Shipped." }]);
  });

  it("offers the selected card's Move dialog as a command", async () => {
    source();
    show("/?view=board&selected=t-6");
    await render(WorkRoute);
    await expect.element(column("Parked").getByRole("listitem")).toHaveLength(1);
    await expect.poll(() => registeredCommands().find((command) => command.id === "work:move")?.keys).toEqual(["m"]);
    registeredCommands().find((command) => command.id === "work:move")?.run();
    const dialog = screen.getByRole("dialog", { name: "Move Task t-6" });
    await expect.element(dialog.getByRole("button", { name: "Pending" })).toBeVisible();
  });

  it("refuses a drop no task command makes, and the card stays", async () => {
    const { calls } = source();
    show("/?view=board");
    await render(WorkRoute);
    const card = column("Parked").getByRole("listitem").filter({ hasText: "Task t-6" });
    await expect.element(card).toBeVisible();
    await userEvent.dragAndDrop(card, column("Completed"));
    await expect.element(screen.getByRole("alert")).toHaveTextContent("No task command moves a task from Parked to Completed. It can move to Pending.");
    await expect.element(column("Parked").getByRole("listitem").filter({ hasText: "Task t-6" })).toBeVisible();
    expect(calls.filter(([name]) => name !== "work")).toEqual([]);
  });

  it("moves a dropped card when the move is allowed", async () => {
    const { calls } = source();
    show("/?view=board");
    await render(WorkRoute);
    const card = column("Pending").getByRole("listitem").filter({ hasText: "Task t-5" });
    await userEvent.dragAndDrop(card, column("Parked"));
    await expect.element(column("Parked").getByRole("listitem").filter({ hasText: "Task t-5" })).toBeVisible();
    expect(calls).toContainEqual(["parkTask", "t-5"]);
  });

  it("keeps a card in place when its move fails or queues", async () => {
    source(sampleList(), { startTask: () => Promise.reject(new DataError(QUEUED, "An agent applies this edit with arcl on the next sync.")) });
    show("/?view=board");
    await render(WorkRoute);
    await screen.getByRole("button", { name: "Move Task t-2" }).click();
    await screen.getByRole("dialog").getByRole("button", { name: "In progress" }).click();
    await expect.element(screen.getByText("Queued for the next sync")).toBeVisible();
    await expect.element(column("Pending").getByRole("listitem").filter({ hasText: "Task t-2" })).toBeVisible();
  });

  it("scrolls a long column inside itself", async () => {
    const list: WorkList = { milestones: [], items: [] };
    for (let index = 0; index < 40; index++) {
      list.items.push({ task: task(`t-${index}`), readiness: { ready: true, blocked: false, reasons: [] }, milestone: null });
    }
    source(list);
    show("/?view=board");
    await render(WorkRoute);
    await expect.element(column("Pending").getByRole("listitem")).toHaveLength(40);
    const pending = column("Pending").element() as HTMLElement;
    expect(pending.scrollHeight).toBeGreaterThan(pending.clientHeight);
    expect(getComputedStyle(pending).overflowY).toBe("auto");
  });
});
