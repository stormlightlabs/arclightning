import { flushSync, mount, tick, unmount } from "svelte";
import { afterEach, describe, expect, it, vi } from "vitest";
import { registeredCommands } from "$lib/commands";
import { DataError, setDataSource, type Changed, type DataSource, type TaskDetail as Detail } from "$lib/data";
import { blockedDetail } from "./fixtures";
import TaskDetail from "./TaskDetail.svelte";

/** `value` as a change that saved with no warnings. */
function changed<T>(value: T): Changed<T> {
  return { value, warnings: [] };
}

/** A data source whose changes are spies; each resolves unless a test says otherwise. */
function stubSource(detail: Detail) {
  const source = {
    work: vi.fn(),
    task: vi.fn(async () => detail),
    createTask: vi.fn(),
    updateTask: vi.fn(async () => changed(detail.task)),
    startTask: vi.fn(async () => changed(detail.task)),
    parkTask: vi.fn(async () => changed(detail.task)),
    unparkTask: vi.fn(async () => changed(detail.task)),
    handOffTask: vi.fn(async () => changed(detail.task)),
    completeTask: vi.fn(async () =>
      changed({ task: detail.task, completed_subtasks: [] as string[], unblocked: [] as string[] }),
    ),
  };
  setDataSource(source as unknown as DataSource);
  return source;
}

let mounted: ReturnType<typeof mount> | null = null;

/** The last value the view wrote to its bindable `unsaved` prop. */
let unsaved = false;

/** Mount the view on `detail`, and return its root element. */
function view(detail: Detail) {
  const target = document.createElement("div");
  document.body.append(target);
  unsaved = false;
  const props = {
    detail,
    workHref: "/",
    onchanged: async () => {},
    get unsaved() {
      return unsaved;
    },
    set unsaved(value: boolean) {
      unsaved = value;
    },
  };
  mounted = mount(TaskDetail, { target, props });
  flushSync();
  return target;
}

/** The button whose visible label is `label`. */
function button(root: HTMLElement, label: string): HTMLButtonElement {
  const found = [...root.querySelectorAll("button")].find((element) => element.textContent?.trim() === label);
  if (found === undefined) throw new Error(`no ${label} button`);
  return found;
}

/** Wait for pending promises and the updates they cause. */
async function settle() {
  for (let round = 0; round < 5; round += 1) {
    await Promise.resolve();
    await tick();
  }
  flushSync();
}

function inProgress(): Detail {
  const detail = blockedDetail();
  detail.task.status = "in_progress";
  return detail;
}

afterEach(() => {
  if (mounted !== null) void unmount(mounted);
  mounted = null;
  document.body.innerHTML = "";
});

describe("TaskDetail changes", () => {
  it("shows a refused change as its message and code next to the actions", async () => {
    const source = stubSource(inProgress());
    source.parkTask.mockRejectedValueOnce(new DataError("invalid_transition", "the task is already parked"));
    const root = view(inProgress());
    button(root, "Park").click();
    await settle();
    const actions = root.querySelector('[aria-label="Task actions"]')!;
    const refusal = actions.parentElement!.textContent!;
    expect(refusal).toContain("Could not park this task");
    expect(refusal).toContain("the task is already parked (invalid_transition)");
  });

  it("lists the tasks a completion unblocked as links", async () => {
    const source = stubSource(inProgress());
    source.completeTask.mockResolvedValueOnce(
      changed({ task: inProgress().task, completed_subtasks: [], unblocked: ["t-10.5", "t-99"] }),
    );
    const root = view(inProgress());
    button(root, "Complete").click();
    flushSync();
    const form = root.querySelector('form[aria-label="Complete"]')!;
    button(form as HTMLElement, "Complete").click();
    await settle();
    expect(source.completeTask).toHaveBeenCalledWith("t-10.4", {});
    const links = [...root.querySelectorAll('[aria-label="Completed"] a')].map((link) => [
      link.getAttribute("href"),
      link.textContent,
    ]);
    expect(links).toEqual([
      ["/tasks/t-10.5", "T10.5: Document the MCP tools"],
      ["/tasks/t-99", "t-99"],
    ]);
  });

  it("shows a saved change's warnings until they are dismissed", async () => {
    const source = stubSource(inProgress());
    const warning = { code: "workspace_sync", message: "the change is saved, but the workspace was not updated" };
    source.parkTask.mockResolvedValueOnce({ value: inProgress().task, warnings: [warning] });
    const root = view(inProgress());
    button(root, "Park").click();
    await settle();
    const notice = root.querySelector('.arcl-feedback[data-tone="warning"]');
    expect(notice?.textContent).toContain("The change is saved, with a warning");
    expect(notice?.textContent).toContain("the change is saved, but the workspace was not updated (workspace_sync)");
    expect(notice?.getAttribute("role")).toBe("status");
    // The notice blocks nothing: the next change runs, and Dismiss clears it.
    expect(button(root, "Park").disabled).toBe(false);
    button(root, "Dismiss").click();
    flushSync();
    expect(root.querySelector('.arcl-feedback[data-tone="warning"]')).toBeNull();
  });

  it("clears the last change's warnings when the next change saves without any", async () => {
    const source = stubSource(inProgress());
    source.parkTask.mockResolvedValueOnce({
      value: inProgress().task,
      warnings: [{ code: "blocker_edge", message: "loaded" }],
    });
    const root = view(inProgress());
    button(root, "Park").click();
    await settle();
    expect(root.querySelector('.arcl-feedback[data-tone="warning"]')).not.toBeNull();
    button(root, "Park").click();
    await settle();
    expect(root.querySelector('.arcl-feedback[data-tone="warning"]')).toBeNull();
  });

  it("saves only the fields the edit form changed", async () => {
    const source = stubSource(inProgress());
    const root = view(inProgress());
    button(root, "Edit").click();
    flushSync();
    const title = root.querySelector<HTMLInputElement>('form[aria-labelledby="task-edit"] input')!;
    title.value = "T10.4: Run capture through MCP";
    title.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    button(root, "Save").click();
    await settle();
    expect(source.updateTask).toHaveBeenCalledTimes(1);
    expect(source.updateTask).toHaveBeenCalledWith("t-10.4", { title: "T10.4: Run capture through MCP" });
  });

  for (const [action, label] of [
    ["complete", "Complete"],
    ["handoff", "Hand off"],
  ] as const) {
    it(`choosing ${label} again only refocuses its open form's editor`, async () => {
      const source = stubSource(inProgress());
      const root = view(inProgress());
      const run = () =>
        registeredCommands()
          .find((command) => command.id === `task:${action}`)!
          .run();
      run();
      await settle();
      const form = root.querySelector<HTMLElement>(`form[aria-label="${label}"]`)!;
      const editor = form.querySelector<HTMLElement>(".cm-content")!;
      expect(document.activeElement).toBe(editor);

      editor.blur();
      run();
      button(root.querySelector('[aria-label="Task actions"]')!, label).click();
      await settle();
      expect(source.completeTask).not.toHaveBeenCalled();
      expect(source.handOffTask).not.toHaveBeenCalled();
      expect(root.querySelector(`form[aria-label="${label}"]`)).toBe(form);
      expect(document.activeElement).toBe(editor);

      button(form, label).click();
      await settle();
      expect(action === "complete" ? source.completeTask : source.handOffTask).toHaveBeenCalledTimes(1);
    });
  }

  it("focuses the editor of a form the view opens with", async () => {
    stubSource(inProgress());
    const target = document.createElement("div");
    document.body.append(target);
    mounted = mount(TaskDetail, { target, props: { detail: inProgress(), workHref: "/", initialAction: "complete" } });
    await settle();
    expect(document.activeElement).toBe(target.querySelector('form[aria-label="Complete"] .cm-content'));
  });

  it("sends nothing on a second click while a change runs", async () => {
    const source = stubSource(inProgress());
    let finish = () => {};
    source.parkTask.mockImplementationOnce(
      () => new Promise((resolve) => (finish = () => resolve(changed(inProgress().task)))),
    );
    const root = view(inProgress());
    button(root, "Park").click();
    flushSync();
    button(root, "Park").click();
    button(root, "Hand off").click();
    flushSync();
    expect(source.parkTask).toHaveBeenCalledTimes(1);
    expect(root.querySelector('form[aria-label="Hand off"]')).toBeNull();
    finish();
    await settle();
    expect(source.parkTask).toHaveBeenCalledTimes(1);
  });

  it("reports unsaved input in an open form", async () => {
    stubSource(inProgress());
    const root = view(inProgress());
    button(root, "Edit").click();
    flushSync();
    expect(unsaved).toBe(false);
    const title = root.querySelector<HTMLInputElement>('form[aria-labelledby="task-edit"] input')!;
    title.value = "Changed";
    title.dispatchEvent(new Event("input", { bubbles: true }));
    await settle();
    expect(unsaved).toBe(true);
    button(root, "Cancel").click();
    flushSync();
    expect(unsaved).toBe(false);
    button(root, "Hand off").click();
    flushSync();
    expect(root.querySelector('form[aria-label="Hand off"]')).not.toBeNull();
    expect(unsaved).toBe(false);
  });
});
