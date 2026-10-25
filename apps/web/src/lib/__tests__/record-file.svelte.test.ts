import { page as screen, userEvent } from "vitest/browser";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import "@arclightning/ui/styles.css";
import { DataError, setDataSource, type DataSource, type RecordSource } from "$lib/data";
import { blockedDetail } from "$lib/task/fixtures";
import TaskRoute from "../../routes/tasks/[id]/+page.svelte";
import { reset, show } from "./app.svelte";
import { fakeSource } from "./source";

vi.mock("$app/state", async () => ({ page: (await import("./app.svelte")).page }));
vi.mock("$app/navigation", async () => {
  const app = await import("./app.svelte");
  return { afterNavigate: app.afterNavigate, goto: app.goto };
});

const FILE: RecordSource = {
  id: "t-10.4",
  kind: "task",
  path: "tasks/t-10.4.md",
  source: '+++\nid = "t-10.4"\ntitle = "T10.4"\n+++\n\nBody\n',
  revision: "r1",
};

function source(answers: Partial<DataSource> = {}) {
  const made = fakeSource({
    task: async () => blockedDetail(),
    recordSource: async () => FILE,
    saveRecordSource: async (_id, edit) => ({ value: { ...FILE, source: edit.source, revision: "r2" }, warnings: [] }),
    ...answers,
  });
  setDataSource(made.source);
  return made;
}

/** Replace the raw editor's text with `text`. */
async function type(text: string) {
  const editor = screen.getByRole("textbox", { name: "Record file" });
  await expect.element(editor).toHaveTextContent("title");
  (editor.element() as HTMLElement).focus();
  await userEvent.keyboard("{Control>}a{/Control}");
  await userEvent.keyboard(text);
}

beforeEach(() => {
  reset();
  show("/tasks/t-10.4");
});

describe("Raw record file editor", () => {
  it("loads the task's file with TOML front matter and saves an edit with its revision", async () => {
    const { calls } = source();
    await render(TaskRoute);
    await screen.getByRole("button", { name: "Edit file" }).click();
    await expect.element(screen.getByText("tasks/t-10.4.md")).toBeVisible();
    const editor = screen.getByRole("textbox", { name: "Record file" });
    await expect.element(editor).toHaveTextContent('id = "t-10.4"');
    // The front matter is highlighted as TOML: its keys are styled spans.
    expect([...editor.element().querySelectorAll("span")].some((span) => span.textContent === "title")).toBe(true);
    const save = screen.getByRole("button", { name: "Save file" });
    await expect.element(save).toBeDisabled();
    await type("+++\nid = \"t-10.4\"\ntitle = \"Renamed\"\n+++\n");
    await save.click();
    await expect.element(screen.getByRole("textbox", { name: "Record file" })).not.toBeInTheDocument();
    expect(calls).toContainEqual(["saveRecordSource", "t-10.4", { source: '+++\nid = "t-10.4"\ntitle = "Renamed"\n+++\n', revision: "r1" }]);
  });

  it("shows the CLI's refusal and keeps the edit, with a reload for a stale file", async () => {
    let refusals = 0;
    source({
      saveRecordSource: async () => {
        refusals++;
        throw new DataError("conflict", "workspace file `tasks/t-10.4.md` changed since it was opened; reload it and make the edit again");
      },
    });
    await render(TaskRoute);
    await screen.getByRole("button", { name: "Edit file" }).click();
    await type("+++\ntitle = \n+++\n");
    await screen.getByRole("button", { name: "Save file" }).click();
    await expect.element(screen.getByRole("alert")).toHaveTextContent("changed since it was opened");
    await expect.element(screen.getByRole("textbox", { name: "Record file" })).toHaveTextContent("title =");
    expect(refusals).toBe(1);
    await screen.getByRole("button", { name: "Reload the file" }).click();
    await expect.element(screen.getByRole("textbox", { name: "Record file" })).toHaveTextContent('title = "T10.4"');
  });

  it("is not offered by a source that cannot save a file, as in the artifact build", async () => {
    // `fakeSource` offers every method; take the raw file methods away.
    const { source: base } = fakeSource({ task: async () => blockedDetail() });
    setDataSource(new Proxy(base, { get: (target, key) => (key === "recordSource" || key === "saveRecordSource" ? undefined : Reflect.get(target, key)) }));
    await render(TaskRoute);
    await expect.element(screen.getByRole("button", { name: "Edit", exact: true })).toBeVisible();
    await expect.element(screen.getByRole("button", { name: "Edit file" })).not.toBeInTheDocument();
  });
});
