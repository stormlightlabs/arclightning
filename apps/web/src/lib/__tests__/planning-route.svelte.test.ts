import { page as screen, userEvent } from "vitest/browser";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import "@arclightning/ui/styles.css";
import { registeredCommands } from "$lib/commands";
import { setDataSource, type DataSource } from "$lib/data";
import { planTemplate } from "$lib/planning";
import { planning } from "$lib/planning/fixtures";
import PlanningRoute from "../../routes/planning/+page.svelte";
import { goto, reset } from "./app.svelte";
import { fakeSource } from "./source";

vi.mock("$app/state", async () => ({ page: (await import("./app.svelte")).page }));
vi.mock("$app/navigation", async () => {
  const app = await import("./app.svelte");
  return { afterNavigate: app.afterNavigate, goto: app.goto };
});

function source(answers: Partial<DataSource> = {}) {
  const made = fakeSource({
    planning: async () => planning(),
    createSpec: async (input) => ({ value: { id: "s-9", title: input.title, body: "", acceptance_criteria: "", status: "open" }, warnings: [] }),
    createPlan: async (input) => ({ value: { id: "pl-9", spec_id: input.spec, title: input.title, body: "", status: "open" }, warnings: [] }),
    applyPlan: async (id) => ({
      value: { plan: { id, spec_id: "s-1", title: "plan pl-1", body: "", status: "open" }, phases: [], tasks: [] },
      warnings: [{ code: "workspace_sync", message: "the workspace was not updated" }],
    }),
    ...answers,
  });
  setDataSource(made.source);
  return made;
}

beforeEach(reset);

describe("Planning route", () => {
  it("shows each milestone with its status, progress and ready count", async () => {
    source();
    await render(PlanningRoute);
    const first = screen.getByRole("button", { name: /phase ph-1/ });
    await expect.element(first).toHaveTextContent("1 of 4 done");
    await expect.element(first).toHaveTextContent("1 ready · 1 blocked");
    await expect.element(first.getByRole("img", { name: "Open" })).toBeVisible();
    await expect.element(screen.getByRole("button", { name: /phase ph-2/ }).getByRole("img", { name: "Done" })).toBeVisible();
    await expect.element(screen.getByRole("button", { name: /plan pl-1/ })).toHaveTextContent("Plan · 2 of 6 done");
    // A closed spec starts collapsed; a spec with no plan says so.
    await expect.element(screen.getByRole("button", { name: /spec s-2/ })).toHaveAttribute("aria-expanded", "false");
    await expect.element(screen.getByRole("button", { name: /plan pl-2/ })).not.toBeInTheDocument();
    await expect.element(screen.getByText("No plans yet. Add one with New plan.")).toBeVisible();
  });

  it("opens a milestone's planning tree below it, whose tasks open the Task view", async () => {
    source();
    await render(PlanningRoute);
    const row = screen.getByRole("button", { name: /phase ph-1/ });
    await row.click();
    await expect.element(row).toHaveAttribute("aria-expanded", "true");
    const tree = screen.getByRole("navigation", { name: "Tasks in phase ph-1" });
    await expect.element(tree.getByRole("button", { name: "task t-3 task" })).toBeVisible();
    await tree.getByRole("button", { name: "task t-2 task" }).click();
    expect(goto).toHaveBeenCalledWith("/tasks/t-2");
    await row.click();
    await expect.element(tree).not.toBeInTheDocument();
  });

  it("moves with j and k and opens the selected row with Enter", async () => {
    source();
    await render(PlanningRoute);
    await expect.element(screen.getByRole("button", { name: /phase ph-1/ })).toBeVisible();
    await userEvent.keyboard("jjj");
    const row = screen.getByRole("button", { name: /phase ph-1/ });
    await expect.element(row).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await expect.element(row).toHaveAttribute("aria-expanded", "true");
    // The repeats of a held Enter on the focused row do not toggle it back.
    await userEvent.keyboard("{Enter>2/}");
    await expect.element(row).toHaveAttribute("aria-expanded", "false");
    const repeat = new KeyboardEvent("keydown", { key: "Enter", repeat: true, bubbles: true, cancelable: true });
    row.element().dispatchEvent(repeat);
    expect(repeat.defaultPrevented).toBe(true);
    await userEvent.keyboard("{Enter}");
    await expect.element(row).toHaveAttribute("aria-expanded", "true");
    await userEvent.keyboard("kk");
    await expect.element(screen.getByRole("button", { name: /spec s-1/ })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await expect.element(row).not.toBeInTheDocument();
  });

  it("creates a spec, then a plan on it, then applies a plan document, from the browser", async () => {
    const { calls } = source();
    await render(PlanningRoute);
    await screen.getByRole("button", { name: "New spec" }).click();
    await expect.element(screen.getByRole("textbox", { name: "Title" })).toHaveFocus();
    await userEvent.keyboard("Browser spec{Enter}");
    await expect.element(screen.getByRole("status")).toHaveTextContent("Created the spec “Browser spec”.");

    await expect.element(screen.getByRole("button", { name: /spec s-1/ })).toBeVisible();
    await userEvent.click(screen.getByRole("button", { name: /spec s-1/ }));
    registeredCommands().find((command) => command.id === "planning:new-plan")?.run();
    await expect.element(screen.getByRole("heading", { name: "New plan" })).toBeVisible();
    await userEvent.keyboard("Delivery{Enter}");
    await expect.element(screen.getByRole("status")).toHaveTextContent("Created the plan “Delivery”.");

    await screen.getByRole("button", { name: "Apply plan" }).first().click();
    await expect.element(screen.getByRole("textbox", { name: /Plan document/ })).toHaveValue(planTemplate());
    await screen.getByRole("button", { name: "Apply", exact: true }).click();
    await expect.element(screen.getByText("the workspace was not updated (workspace_sync)")).toBeVisible();

    expect(calls.filter(([name]) => name !== "planning")).toEqual([
      ["createSpec", { title: "Browser spec" }],
      ["createPlan", { title: "Delivery", spec: "s-1" }],
      ["applyPlan", "pl-1", planTemplate()],
    ]);
    // Each change reloads the view in place.
    expect(calls.filter(([name]) => name === "planning")).toHaveLength(4);
  });

  it("offers New plan on a selected spec and Apply plan on a selected plan, both on a", async () => {
    source();
    await render(PlanningRoute);
    await expect.element(screen.getByRole("button", { name: /spec s-1/ })).toBeVisible();
    const added = () => registeredCommands().filter((command) => command.keys?.[0] === "a").map((command) => command.id);
    await userEvent.keyboard("j");
    expect(added()).toEqual(["planning:new-plan"]);
    await userEvent.keyboard("j");
    expect(added()).toEqual(["planning:apply"]);
    await userEvent.keyboard("j");
    expect(added()).toEqual([]);
  });

  it.each([
    { kind: "spec", heading: "New spec", field: /Acceptance criteria/ },
    { kind: "plan", heading: "New plan", field: /Description/ },
    { kind: "apply", heading: "Apply plan", field: /Plan document/ },
  ])("keeps the $kind form open while it holds edits", async ({ kind, heading, field }) => {
    source();
    await render(PlanningRoute);
    await expect.element(screen.getByRole("button", { name: /spec s-1/ })).toBeVisible();
    if (kind === "spec") await screen.getByRole("button", { name: "New spec" }).click();
    else await screen.getByRole("button", { name: kind === "plan" ? "New plan" : "Apply plan" }).first().click();
    const form = screen.getByRole("form", { name: heading });
    const input = form.getByRole("textbox", { name: field });
    await input.fill("Edited");
    // Escape with focus in the field leaves the edits in place.
    await userEvent.keyboard("{Escape}");
    await expect.element(input).toHaveValue("Edited");
    // n and a take focus back to the form instead of replacing it.
    // j selects the spec, then the plan, so New plan and then Apply plan are offered.
    const opens = new Set<string>();
    for (const step of ["j", "j"]) {
      (document.activeElement as HTMLElement).blur();
      await userEvent.keyboard(step);
      for (const command of registeredCommands().filter(({ id }) => ["planning:new-spec", "planning:new-plan", "planning:apply"].includes(id))) {
        opens.add(command.id);
        (document.activeElement as HTMLElement).blur();
        command.run();
        await expect.element(form).toBeVisible();
        await expect.element(form.getByRole("textbox").first()).toHaveFocus();
      }
    }
    expect([...opens].sort()).toEqual(["planning:apply", "planning:new-plan", "planning:new-spec"]);
    await expect.element(input).toHaveValue("Edited");
    await form.getByRole("button", { name: "Cancel" }).click();
    await expect.element(form).not.toBeInTheDocument();
  });

  it("keeps the form whose request is in flight open until the reply", async () => {
    let settle!: (value: Awaited<ReturnType<DataSource["applyPlan"]>>) => void;
    source({ applyPlan: () => new Promise((resolve) => (settle = resolve)) });
    await render(PlanningRoute);
    await screen.getByRole("button", { name: "Apply plan" }).first().click();
    await screen.getByRole("button", { name: "Apply", exact: true }).click();
    const form = screen.getByRole("form", { name: "Apply plan" });
    (document.activeElement as HTMLElement).blur();
    // Escape and New spec leave the form that sent the request in place.
    await userEvent.keyboard("{Escape}");
    registeredCommands().find((command) => command.id === "planning:new-spec")?.run();
    await expect.element(form).toBeVisible();
    await expect.element(screen.getByRole("form", { name: "New spec" })).not.toBeInTheDocument();
    settle({ value: { plan: { id: "pl-1", spec_id: "s-1", title: "plan pl-1", body: "", status: "open" }, phases: [], tasks: [] }, warnings: [] });
    await expect.element(screen.getByRole("status")).toHaveTextContent("Applied “plan pl-1”");
    await expect.element(form).not.toBeInTheDocument();
  });

  it("says when there is no spec yet", async () => {
    source({ planning: async () => ({ tree: [], work: { milestones: [], items: [] } }) });
    await render(PlanningRoute);
    await expect.element(screen.getByRole("heading", { name: "No specs yet" })).toBeVisible();
  });
});
