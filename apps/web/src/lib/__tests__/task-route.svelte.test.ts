import { flushSync } from "svelte";
import { page as screen, userEvent } from "vitest/browser";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import { MarkdownEditor } from "@arclightning/ui";
import "@arclightning/ui/styles.css";
import { DataError, setDataSource, type DataSource, type TaskDetail } from "$lib/data";
import { taskKeyAction } from "$lib/task";
import { blockedDetail } from "$lib/task/fixtures";
import TaskRoute from "../../routes/tasks/[id]/+page.svelte";
import { arrive, goto, navigate, reset, show } from "./app.svelte";

vi.mock("$app/state", async () => ({ page: (await import("./app.svelte")).page }));
vi.mock("$app/navigation", async () => {
  const app = await import("./app.svelte");
  return { afterNavigate: app.afterNavigate, goto: app.goto };
});

/** A read the test answers when it chooses. */
interface Pending {
  signal: AbortSignal | undefined;
  answer(): void;
  fail(message: string): void;
}

/** A change these tests never make. */
const unused = () => Promise.reject(new Error("not used in these tests"));

/** A data source whose task reads wait for the test, and ignore their signal. */
function slowSource() {
  const reads: Record<string, Pending[]> = {};
  const source: DataSource = {
    work: () => new Promise(() => {}),
    task: (id, signal) =>
      new Promise<TaskDetail>((resolve, reject) => {
        (reads[id] ??= []).push({
          signal,
          answer: () => resolve(detailOf(id)),
          fail: (message) => reject(new DataError("unavailable", message)),
        });
      }),
    createTask: unused,
    updateTask: unused,
    startTask: unused,
    parkTask: unused,
    unparkTask: unused,
    handOffTask: unused,
    completeTask: unused,
  };
  return { source, reads };
}

function detailOf(id: string): TaskDetail {
  const detail = blockedDetail();
  return { ...detail, task: { ...detail.task, id, title: `Task ${id}` } };
}

/** Let settled reads report and the view update. */
async function settle() {
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}

beforeEach(reset);

describe("Task route", () => {
  it("shows the task navigated to when the one before answers last", async () => {
    const { source, reads } = slowSource();
    setDataSource(source);
    show("/tasks/a");
    await render(TaskRoute);
    navigate("/tasks/b");
    flushSync();
    reads.b[0].answer();
    await expect.element(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Task b");
    reads.a[0].answer();
    await settle();
    await expect.element(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Task b");
    expect(reads.a[0].signal?.aborted).toBe(true);
  });

  it("stops the read in flight when it unmounts", async () => {
    const { source, reads } = slowSource();
    setDataSource(source);
    show("/tasks/a");
    const view = await render(TaskRoute);
    view.unmount();
    expect(reads.a[0].signal?.aborted).toBe(true);
  });

  it("retries a failed read", async () => {
    const { source, reads } = slowSource();
    setDataSource(source);
    show("/tasks/a");
    await render(TaskRoute);
    reads.a[0].fail("arcl serve stopped");
    await expect.element(screen.getByText("arcl serve stopped")).toBeVisible();
    await screen.getByRole("button", { name: "Retry" }).click();
    reads.a[1].answer();
    await expect.element(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Task a");
  });

  it("titles the page Task until the task loads, then with its title", async () => {
    const { source, reads } = slowSource();
    setDataSource(source);
    show("/tasks/a");
    await render(TaskRoute);
    expect(document.title).toBe("Task · Arc Lightning");
    reads.a[0].answer();
    await expect.element(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Task a");
    expect(document.title).toBe("Task a · Arc Lightning");
  });

  it("returns to Work with the filters it was opened from after links to other tasks", async () => {
    const { source } = slowSource();
    setDataSource(source);
    show("/tasks/a");
    await render(TaskRoute);
    arrive("/?status=pending&priority=high&selected=a");
    navigate("/tasks/b");
    flushSync();
    await userEvent.keyboard("{Escape}");
    expect(goto).toHaveBeenCalledWith("/?status=pending&priority=high&selected=b");
  });

  it("takes ] from inside the read-only Markdown source", async () => {
    const { source, reads } = slowSource();
    setDataSource(source);
    show("/tasks/a");
    await render(TaskRoute);
    reads.a[0].answer();
    await screen.getByRole("button", { name: "View Markdown" }).click();
    const panel = screen.getByRole("button", { name: /Properties/ });
    await expect.element(panel).toHaveAttribute("aria-expanded", "true");
    screen.getByRole("textbox", { name: "Description Markdown" }).element().focus();
    await userEvent.keyboard("]");
    await expect.element(panel).toHaveAttribute("aria-expanded", "false");
  });
});

describe("taskKeyAction in the Markdown editor", () => {
  /** What `taskKeyAction` makes of `key` pressed in an editor that is `readonly` or not. */
  async function actionIn(readonly: boolean, key: string) {
    await render(MarkdownEditor, { value: "Some text.", label: "Source", readonly });
    const content = screen.getByRole("textbox", { name: "Source" }).element();
    const event = new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true });
    content.dispatchEvent(event);
    return taskKeyAction(event);
  }

  it("takes ] when the editor is read-only", async () => {
    expect(await actionIn(true, "]")).toBe("properties");
  });

  it("leaves ] to a writable editor", async () => {
    expect(await actionIn(false, "]")).toBeNull();
  });
});
