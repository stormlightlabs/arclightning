import { page as screen, userEvent } from "vitest/browser";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import "@arclightning/ui/styles.css";
import { registeredCommands } from "$lib/commands/registry.svelte";
import { DataError, type Capture, type CaptureSummary, type DataSource } from "$lib/data/types";
import { setDataSource } from "$lib/data/source";
import { blockedDetail } from "$lib/task/fixtures";
import InboxRoute from "../../routes/inbox/+page.svelte";
import { reset } from "./app.svelte";
import { fakeSource } from "./fake-source";

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
    work: async () => ({
      milestones: [],
      items: [{ task: { ...task, id: "t-18", title: "T18: Serve the web app" }, readiness, milestone: null }],
    }),
    promoteCapture: async (_id, input) => ({
      value: { kind: input.to, id: `${input.to}-1`, title: input.title ?? "" },
      warnings: [],
    }),
    discardCapture: async (id) => ({ value: { ...captureOf(id), status: "discarded" }, warnings: [] }),
    ...answers,
  });
  setDataSource(made.source);
  return made;
}

/** A promise the test settles by hand. */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

/** A capture ID whose ULID says it was created at `ms`. */
function idAt(ms: number): string {
  const crockford = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let time = "";
  for (let rest = ms, index = 0; index < 10; index++, rest = Math.floor(rest / 32)) time = crockford[rest % 32] + time;
  return `arcl-c-${time}${"0".repeat(16)}`;
}

beforeEach(reset);
afterEach(() => vi.useRealTimers());

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

  it("moves one row on ArrowDown from a focused row, and Home and End reach the ends", async () => {
    source();
    await render(InboxRoute);
    await screen.getByRole("button", { name: summaries[0].title }).click();
    await expect.element(screen.getByText("Body of c-1")).toBeVisible();
    await userEvent.keyboard("{ArrowDown}");
    await expect.element(screen.getByRole("button", { name: "Planned idea" })).toHaveAttribute("aria-current", "true");
    await userEvent.keyboard("{ArrowDown}");
    await expect.element(screen.getByRole("button", { name: "Stray thought" })).toHaveAttribute("aria-current", "true");
    await userEvent.keyboard("{ArrowUp}");
    await expect.element(screen.getByRole("button", { name: "Planned idea" })).toHaveAttribute("aria-current", "true");
    await expect.element(screen.getByRole("button", { name: "Planned idea" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    await expect.element(screen.getByRole("button", { name: "Stray thought" })).toHaveAttribute("aria-current", "true");
    await userEvent.keyboard("{Home}");
    await expect
      .element(screen.getByRole("button", { name: summaries[0].title }))
      .toHaveAttribute("aria-current", "true");
  });

  it("counts the Created column's ages on from the current time", async () => {
    vi.useFakeTimers({ toFake: ["Date", "setInterval", "clearInterval"] });
    vi.setSystemTime(new Date("2026-10-25T12:00:00Z"));
    const fresh = { id: idAt(Date.now() - 10_000), title: "Fresh idea", status: "captured" as const };
    source({
      captures: async () => [fresh],
      capture: async () => ({ ...fresh, body: "Body", created_at: new Date().toISOString() }),
    });
    await render(InboxRoute);
    await expect.element(screen.getByRole("cell", { name: "just now" })).toBeVisible();
    vi.advanceTimersByTime(60_000);
    await expect.element(screen.getByRole("cell", { name: "1m ago" })).toBeVisible();
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
    await expect
      .element(screen.getByRole("status").filter({ hasText: "Promoted" }))
      .toHaveTextContent("Promoted “Show unblocked tasks !high ^T18” to the task “Show unblocked tasks”.");
    await expect.element(screen.getByRole("link", { name: "Open task" })).toHaveAttribute("href", "/tasks/task-1");
    expect(calls).toContainEqual([
      "promoteCapture",
      "c-1",
      { to: "task", title: "Show unblocked tasks", priority: "high", parent: "t-18" },
    ]);
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
    await expect
      .element(screen.getByRole("status").filter({ hasText: "Discarded" }))
      .toHaveTextContent("Discarded “Show unblocked tasks !high ^T18”.");
    expect(calls).toContainEqual(["discardCapture", "c-1"]);
  });

  it("shows the CLI's refusal next to the form and keeps the capture", async () => {
    source({
      promoteCapture: async () => Promise.reject(new DataError("invalid_request", "task title cannot be empty")),
    });
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
    registeredCommands()
      .find((command) => command.id === "inbox:spec")
      ?.run();
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
    registeredCommands()
      .find((command) => command.id === `inbox:${action}`)
      ?.run();
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
    registeredCommands()
      .find((command) => command.id === other)
      ?.run();
    await expect.element(screen.getByRole("heading", { level: 3 })).toHaveTextContent(label);
    await expect.element(screen.getByRole("textbox", { name: "Title" })).toHaveFocus();
    await expect.element(input).toHaveValue("Edited");
    await screen.getByRole("button", { name: "Cancel" }).click();
    await expect.element(screen.getByRole("heading", { level: 3 })).not.toBeInTheDocument();
  });

  it("keeps the selection while the form holds edits, and takes focus back to it", async () => {
    source();
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Make a note" }).click();
    const title = screen.getByRole("textbox", { name: "Title" });
    await title.fill("Edited");
    (document.activeElement as HTMLElement).blur();
    await userEvent.keyboard("j");
    await expect.element(title).toHaveFocus();
    await screen.getByRole("button", { name: "Planned idea" }).click();
    await expect.element(title).toHaveFocus();
    await expect.element(title).toHaveValue("Edited");
    await expect
      .element(screen.getByRole("button", { name: summaries[0].title }))
      .toHaveAttribute("aria-current", "true");
    await expect.element(screen.getByText("Body of c-1")).toBeVisible();
  });

  it("keeps the selection while a promote is in flight, then selects the next capture", async () => {
    const reply = deferred<Awaited<ReturnType<DataSource["promoteCapture"]>>>();
    source({ promoteCapture: () => reply.promise });
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Make a note" }).click();
    await screen.getByRole("button", { name: "Create note" }).click();
    (document.activeElement as HTMLElement).blur();
    await userEvent.keyboard("j");
    await screen.getByRole("button", { name: "Stray thought" }).click();
    // Escape does not close the form whose request is in flight.
    await userEvent.keyboard("{Escape}");
    await expect.element(screen.getByRole("heading", { level: 3, name: "Make a note" })).toBeVisible();
    await expect
      .element(screen.getByRole("button", { name: summaries[0].title }))
      .toHaveAttribute("aria-current", "true");
    reply.resolve({ value: { kind: "note", id: "n-1", title: summaries[0].title }, warnings: [] });
    await expect.element(screen.getByText("2 open captures")).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "Planned idea" })).toHaveAttribute("aria-current", "true");
  });

  it("keeps the selection while a discard is in flight, and shows its failure under its capture", async () => {
    const reply = deferred<Awaited<ReturnType<DataSource["discardCapture"]>>>();
    source({ discardCapture: () => reply.promise });
    await render(InboxRoute);
    await screen.getByRole("button", { name: "Discard", exact: true }).click();
    await screen.getByRole("dialog").getByRole("button", { name: "Discard" }).click();
    (document.activeElement as HTMLElement).blur();
    await userEvent.keyboard("j");
    await screen.getByRole("button", { name: "Stray thought" }).click();
    reply.reject(new DataError("unavailable", "the server stopped"));
    const message = screen.getByText("the server stopped (unavailable)");
    await expect.element(message).toBeVisible();
    await expect
      .element(screen.getByRole("button", { name: summaries[0].title }))
      .toHaveAttribute("aria-current", "true");
    await userEvent.keyboard("j");
    await expect.element(screen.getByText("Body of c-2")).toBeVisible();
    await expect.element(message).not.toBeInTheDocument();
  });

  it.each([
    { code: "not_found", action: "promote" },
    { code: "conflict", action: "discard" },
  ])("reloads the list when a $action fails with $code", async ({ code, action }) => {
    let reads = 0;
    const refuse = async () => Promise.reject(new DataError(code, "capture `c-1` is not open"));
    const { calls } = source({
      captures: async () => (reads++ === 0 ? summaries : summaries.slice(1)),
      promoteCapture: refuse,
      discardCapture: refuse,
    });
    await render(InboxRoute);
    if (action === "promote") {
      await screen.getByRole("button", { name: "Make a note" }).click();
      await screen.getByRole("button", { name: "Create note" }).click();
    } else {
      await screen.getByRole("button", { name: "Discard", exact: true }).click();
      await screen.getByRole("dialog").getByRole("button", { name: "Discard" }).click();
    }
    await expect
      .element(screen.getByText(`changed elsewhere: capture \`c-1\` is not open (${code})`, { exact: false }))
      .toBeVisible();
    await expect.element(screen.getByText("2 open captures")).toBeVisible();
    await expect.element(screen.getByText("Body of c-2")).toBeVisible();
    expect(calls.filter(([name]) => name === "captures")).toHaveLength(2);
  });

  it("says when the Inbox is empty", async () => {
    source({ captures: async () => [] });
    await render(InboxRoute);
    await expect.element(screen.getByRole("heading", { name: "The Inbox is empty" })).toBeVisible();
  });

  it("lists captures in a table with the time each was created", async () => {
    const ulid = "arcl-c-01M5QSTZR7D5Y37H635KD8NA44";
    source({ captures: async () => [...summaries, { id: ulid, title: "Dated idea", status: "captured" }] });
    await render(InboxRoute);
    const table = screen.getByRole("table");
    await expect.element(table.getByRole("columnheader", { name: "Title" })).toBeVisible();
    await expect.element(table.getByRole("columnheader", { name: "Created" })).toBeVisible();
    const time = table.getByRole("row").filter({ hasText: "Dated idea" }).element().querySelector("time");
    expect(time?.getAttribute("datetime")).toBe("2026-10-24T20:32:35.079Z");
    // A click anywhere in a row selects its capture.
    await table.getByRole("row").filter({ hasText: "Dated idea" }).getByRole("cell").last().click();
    await expect.element(screen.getByRole("button", { name: "Dated idea" })).toHaveAttribute("aria-current", "true");
    await expect.element(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Dated idea");
  });
});

describe("Inbox layout", () => {
  const many: CaptureSummary[] = Array.from({ length: 40 }, (_, index) => ({
    id: `c-${index + 1}`,
    title: `Capture number ${index + 1} with a title long enough to need truncating in its cell`,
    status: "captured",
  }));

  function layout() {
    const box = (element: Element) => element.getBoundingClientRect();
    const columns = document.querySelector(".columns")!;
    const table = screen.getByRole("region", { name: "Captures" }).element() as HTMLElement;
    const triage = screen.getByRole("region", { name: "Triage" }).element();
    return { columns: box(columns), table: box(table), triage: box(triage), scroller: table };
  }

  beforeEach(() => {
    source({
      captures: async () => many,
      capture: async (id) => ({
        id,
        title: `Title of ${id}`,
        status: "captured",
        body: "",
        created_at: "2026-10-23T03:51:43Z",
      }),
    });
  });
  afterEach(() => screen.viewport(1024, 768));

  it("shows the selected capture beside the list, which fits its box and scrolls inside it", async () => {
    // 660 px is wide enough for both columns, and narrow enough that the
    // list used to wrap above the capture.
    await screen.viewport(660, 800);
    await render(InboxRoute);
    await expect.element(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Title of c-1");
    const { columns, table, triage, scroller } = layout();
    expect(triage.left).toBeGreaterThanOrEqual(table.right - 1);
    expect(Math.abs(triage.top - table.top)).toBeLessThan(2);
    expect(table.left).toBeGreaterThanOrEqual(columns.left);
    expect(triage.right).toBeLessThanOrEqual(columns.right);
    expect(table.bottom).toBeLessThanOrEqual(columns.bottom);
    expect(scroller.scrollHeight).toBeGreaterThan(scroller.clientHeight);
    expect(scroller.scrollWidth).toBeLessThanOrEqual(scroller.clientWidth);
    // j moves the selection down the table and keeps its row in view.
    for (let step = 0; step < 30; step++) await userEvent.keyboard("j");
    await expect.element(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Title of c-31");
    const row = screen
      .getByRole("button", { name: /^Capture number 31 / })
      .element()
      .getBoundingClientRect();
    expect(row.top).toBeGreaterThanOrEqual(table.top);
    expect(row.bottom).toBeLessThanOrEqual(table.bottom + 1);
  });

  it("stacks the selected capture below the list in a narrow window", async () => {
    await screen.viewport(390, 844);
    await render(InboxRoute);
    await expect.element(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Title of c-1");
    const { columns, table, triage, scroller } = layout();
    expect(triage.top).toBeGreaterThanOrEqual(table.bottom - 1);
    expect(Math.round(table.width)).toBe(Math.round(columns.width - 2));
    expect(scroller.scrollHeight).toBeGreaterThan(scroller.clientHeight);
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(document.documentElement.clientWidth);
  });
});
