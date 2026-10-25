import { page as screen, userEvent } from "vitest/browser";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import "@arclightning/ui/styles.css";
import { registeredCommands } from "$lib/commands";
import { DataError, setDataSource, type Capture, type CaptureSummary, type DataSource } from "$lib/data";
import { blockedDetail } from "$lib/task/fixtures";
import InboxRoute from "../../routes/inbox/+page.svelte";
import { reset } from "./app.svelte";
import { fakeSource } from "./source";

vi.mock("$app/state", async () => ({ page: (await import("./app.svelte")).page }));
vi.mock("$app/navigation", async () => {
  const app = await import("./app.svelte");
  return { afterNavigate: app.afterNavigate, goto: app.goto };
});

const summaries: CaptureSummary[] = [
  { id: "c-1", title: "Show unblocked tasks !high ^T18", status: "captured" },
  { id: "c-2", title: "Planned idea", status: "captured" },
  { id: "c-3", title: "Stray thought", status: "captured" },
];

function captureOf(id: string): Capture {
  const summary = summaries.find((capture) => capture.id === id)!;
  return { ...summary, body: `Body of ${id}`, created_at: "2026-10-23T03:51:43Z" };
}

function source(answers: Partial<DataSource> = {}) {
  const { task, readiness } = blockedDetail();
  const made = fakeSource({
    captures: async () => summaries,
    capture: async (id) => captureOf(id),
    work: async () => ({ milestones: [], items: [{ task: { ...task, id: "t-18", title: "T18: Serve the web app" }, readiness, milestone: null }] }),
    promoteCapture: async (_id, input) => ({ value: { kind: input.to, id: `${input.to}-1`, title: input.title ?? "" }, warnings: [] }),
    discardCapture: async (id) => ({ value: { ...captureOf(id), status: "discarded" }, warnings: [] }),
    ...answers,
  });
  setDataSource(made.source);
  return made;
}

beforeEach(reset);

describe("Inbox route", () => {
  it("lists open captures, shows the selected one's body, and moves with j and k", async () => {
    source();
    await render(InboxRoute);
    await expect.element(screen.getByText("3 open captures")).toBeVisible();
    await expect.element(screen.getByRole("heading", { level: 2 })).toHaveTextContent(summaries[0].title);
    await expect.element(screen.getByText("Body of c-1")).toBeVisible();
    await expect.element(screen.getByText("Capture · Oct 23")).toBeVisible();
    await userEvent.keyboard("j");
    await expect.element(screen.getByText("Body of c-2")).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "Planned idea" })).toHaveAttribute("aria-current", "true");
    await userEvent.keyboard("k");
    await expect.element(screen.getByText("Body of c-1")).toBeVisible();
  });

  it("promotes a capture to a task with the fields its tokens set, then selects the next", async () => {
    const { calls } = source();
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Promote to task" }).click();
    const title = screen.getByRole("textbox", { name: "Title" });
    await expect.element(title).toHaveValue("Show unblocked tasks");
    await expect.element(title).toHaveFocus();
    await expect.element(screen.getByRole("combobox", { name: /Priority/ })).toHaveTextContent("High");
    await screen.getByRole("button", { name: "Create task" }).click();
    await expect.element(screen.getByRole("status").filter({ hasText: "Promoted" })).toHaveTextContent("Promoted “Show unblocked tasks !high ^T18” to the task “Show unblocked tasks”.");
    await expect.element(screen.getByRole("link", { name: "Open task" })).toHaveAttribute("href", "/tasks/task-1");
    expect(calls).toContainEqual(["promoteCapture", "c-1", { to: "task", title: "Show unblocked tasks", priority: "high", parent: "t-18" }]);
    await expect.element(screen.getByText("2 open captures")).toBeVisible();
    await expect.element(screen.getByText("Body of c-2")).toBeVisible();
  });

  it("promotes a capture to a spec with acceptance criteria, and to a note", async () => {
    const { calls } = source();
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Planned idea" }).click();
    await screen.getByRole("button", { name: "Promote to spec" }).click();
    await screen.getByRole("textbox", { name: /Acceptance criteria/ }).fill("- it works");
    await screen.getByRole("button", { name: "Create spec" }).click();
    await expect.element(screen.getByRole("link", { name: "Open Planning" })).toHaveAttribute("href", "/planning");
    await screen.getByRole("button", { name: "Make a note" }).click();
    await screen.getByRole("button", { name: "Create note" }).click();
    await expect.element(screen.getByRole("status").filter({ hasText: "Made the note" })).toBeVisible();
    expect(calls.filter(([name]) => name === "promoteCapture")).toEqual([
      ["promoteCapture", "c-2", { to: "spec", title: "Planned idea", acceptance_criteria: "- it works" }],
      ["promoteCapture", "c-3", { to: "note", title: "Stray thought" }],
    ]);
  });

  it("discards a capture once the dialog confirms it", async () => {
    const { calls } = source();
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Discard", exact: true }).click();
    const dialog = screen.getByRole("dialog", { name: "Discard this capture?" });
    await expect.element(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "Discard" }).click();
    await expect.element(screen.getByRole("status").filter({ hasText: "Discarded" })).toHaveTextContent("Discarded “Show unblocked tasks !high ^T18”.");
    expect(calls).toContainEqual(["discardCapture", "c-1"]);
  });

  it("shows the CLI's refusal next to the form and keeps the capture", async () => {
    source({ promoteCapture: async () => Promise.reject(new DataError("invalid_request", "task title cannot be empty")) });
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Make a note" }).click();
    await screen.getByRole("button", { name: "Create note" }).click();
    await expect.element(screen.getByText("task title cannot be empty (invalid_request)")).toBeVisible();
    await expect.element(screen.getByText("3 open captures")).toBeVisible();
  });

  it("offers 1, 2, 3 and Backspace on the selected capture, and Escape closes the form", async () => {
    source();
    await render(InboxRoute);
    await expect.element(screen.getByText("Body of c-1")).toBeVisible();
    const keys = registeredCommands()
      .filter((command) => command.scope === "selection")
      .map((command) => [command.id, command.keys]);
    expect(keys).toEqual([
      ["inbox:task", ["1"]],
      ["inbox:spec", ["2"]],
      ["inbox:note", ["3"]],
      ["inbox:discard", ["Backspace"]],
    ]);
    registeredCommands().find((command) => command.id === "inbox:spec")?.run();
    await expect.element(screen.getByRole("heading", { level: 3, name: "Promote to spec" })).toBeVisible();
    await userEvent.keyboard("{Escape}");
    await expect.element(screen.getByRole("heading", { level: 3, name: "Promote to spec" })).not.toBeInTheDocument();
  });

  it.each([
    { action: "task", field: "Title", other: "inbox:note" },
    { action: "spec", field: /Acceptance criteria/, other: "inbox:task" },
    { action: "note", field: "Title", other: "inbox:spec" },
  ])("keeps the $action form open while it holds edits", async ({ action, field, other }) => {
    source();
    await render(InboxRoute);
    await expect.element(screen.getByText("Body of c-1")).toBeVisible();
    registeredCommands().find((command) => command.id === `inbox:${action}`)?.run();
    const heading = screen.getByRole("heading", { level: 3 });
    await expect.element(heading).toBeVisible();
    const label = heading.element().textContent ?? "";
    const input = screen.getByRole("textbox", { name: field });
    await input.fill("Edited");
    // Escape with focus in the field leaves the edits in place.
    await userEvent.keyboard("{Escape}");
    await expect.element(input).toHaveValue("Edited");
    // Another triage key takes focus back to the form instead of replacing it.
    (document.activeElement as HTMLElement).blur();
    registeredCommands().find((command) => command.id === other)?.run();
    await expect.element(screen.getByRole("heading", { level: 3 })).toHaveTextContent(label);
    await expect.element(screen.getByRole("textbox", { name: "Title" })).toHaveFocus();
    await expect.element(input).toHaveValue("Edited");
    await screen.getByRole("button", { name: "Cancel" }).click();
    await expect.element(screen.getByRole("heading", { level: 3 })).not.toBeInTheDocument();
  });

  it("says when the Inbox is empty", async () => {
    source({ captures: async () => [] });
    await render(InboxRoute);
    await expect.element(screen.getByRole("heading", { name: "The Inbox is empty" })).toBeVisible();
  });
});
