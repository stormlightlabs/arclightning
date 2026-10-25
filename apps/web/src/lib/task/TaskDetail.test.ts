import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import { bareDetail, blockedDetail, doneBlockersDetail } from "./fixtures";
import TaskDetail from "./TaskDetail.svelte";

/** The view's HTML without class attributes and hydration comments, which vary by build. */
const show = (props: Partial<{ panelOpen: boolean }> = {}, detail = blockedDetail()) =>
  render(TaskDetail, { props: { detail, workHref: "/?selected=t-10.4", ...props } })
    .body.replace(/ class="[^"]*"/g, "")
    .replace(/<!--[^]*?-->/g, "");

/** The HTML from `start` up to the next `end` after it. */
function slice(body: string, start: string, end: string): string {
  const from = body.indexOf(start);
  expect(from).toBeGreaterThanOrEqual(0);
  return body.slice(from, body.indexOf(end, from + start.length));
}

describe("TaskDetail", () => {
  it("shows each readiness reason with links to the records it names", () => {
    const readiness = slice(show(), 'aria-label="Readiness"', "</section>");
    expect(readiness).toContain("Not ready");
    expect(readiness).toContain("3 reasons");
    expect(readiness).toMatch(/blocked by <a href="\/tasks\/t-10\.3"[^>]*>T10\.3: Add the MCP server<\/a>/);
    expect(readiness).toContain('href="/tasks/t-10"');
    expect(readiness).toContain('href="/tasks/t-17"');
    expect(readiness).toContain("inherited_blocker");
  });

  it("lists direct blockers and blockers inherited from each ancestor, linked", () => {
    const blockers = slice(show(), 'id="task-blockers"', "</section>");
    const direct = slice(blockers, 'aria-label="Direct blockers"', "</ul>");
    expect(direct).toContain('href="/tasks/t-10.3"');
    expect(direct).toContain(">In progress<");
    expect(blockers).toMatch(/Inherited from <a href="\/tasks\/t-10">T10: Add the MCP server<\/a>/);
    expect(slice(blockers, 'aria-label="Inherited from T10: Add the MCP server"', "</ul>")).toContain('href="/tasks/t-17"');
  });

  it("lists completed blockers apart, as done, with their evidence as Markdown", () => {
    const body = show({}, doneBlockersDetail());
    const blocking = slice(body, 'id="task-blockers"', "</section>");
    expect(blocking).not.toContain("t-10.2");
    expect(blocking).not.toContain("t-9");
    const done = slice(body, 'id="task-done-blockers"', "</section>");
    expect(done).toMatch(/href="\/tasks\/t-10\.2">T10\.2: Define the tool schema<\/a>/);
    expect(done).toContain(">Completed<");
    expect(done).toContain("Landed in <strong>#38</strong>.");
    expect(done).toMatch(/T9: Ship the store[^]*Inherited from <a href="\/tasks\/t-10">T10: Add the MCP server<\/a>[^]*No evidence recorded\./);
  });

  it("shows the latest handoff and the evidence as Markdown", () => {
    const activity = slice(show(), 'id="task-activity"', "</section>");
    expect(activity).toContain("Mutations still need the <strong>authorization</strong> check.");
    expect(activity).toContain("Delivered by #41.");
  });

  it("says when there is no handoff, evidence, blocker or dependent", () => {
    const body = show({}, bareDetail());
    expect(body).toContain("Ready to start");
    expect(body).toContain("No handoff recorded.");
    expect(body).toContain("No evidence recorded.");
    expect(body).toContain("No other task waits on this one.");
    expect(body).not.toContain('id="task-blockers"');
    expect(body).not.toContain('id="task-done-blockers"');
  });

  it("renders the body as Markdown with a way to open its source", () => {
    const description = slice(show(), 'id="task-description"', "</section>");
    expect(description).toContain('<li><input type="checkbox" disabled="" /> A run passes through MCP tools only</li>');
    expect(description).toContain('aria-pressed="false"');
    expect(description).toContain("View Markdown");
  });

  it("links the breadcrumb back to the Work list and lists the unblocked task", () => {
    const body = show();
    expect(slice(body, 'aria-label="Breadcrumb"', "</nav>")).toContain('href="/?selected=t-10.4"');
    expect(slice(body, 'id="task-unblocks"', "</section>")).toContain('href="/tasks/t-10.5"');
  });

  it("offers the actions the task's status allows, with Edit", () => {
    const buttons = (status: "pending" | "in_progress" | "parked" | "completed") => {
      const detail = blockedDetail();
      detail.task.status = status;
      return [...slice(show({}, detail), 'aria-label="Task actions"', "</div>").matchAll(/<button[^>]*>\s*<span>([^<]*)<\/span>/g)].map((match) => match[1].trim());
    };
    expect(buttons("pending")).toEqual(["Start", "Park", "Edit"]);
    expect(buttons("in_progress")).toEqual(["Complete", "Hand off", "Park", "Edit"]);
    expect(buttons("parked")).toEqual(["Unpark", "Edit"]);
    expect(buttons("completed")).toEqual(["Edit"]);
  });

  it("marks Complete and Hand off as opening a Markdown form", () => {
    const detail = blockedDetail();
    detail.task.status = "in_progress";
    const actions = slice(show({}, detail), 'aria-label="Task actions"', "</div>");
    for (const label of ["Complete", "Hand off"]) {
      expect(actions).toMatch(new RegExp(`<button[^>]*aria-expanded="false"[^>]*>\\s*<span>${label}<`));
    }
    expect(actions).not.toMatch(/<button[^>]*aria-expanded[^>]*>\s*<span>Park</);
  });

  it("expands or collapses the properties panel with its state", () => {
    const open = show({ panelOpen: true });
    expect(open).toContain('aria-expanded="true"');
    expect(slice(open, "<dl", "</dl>")).not.toContain("hidden");
    expect(open).toContain('href="/tasks/t-10">T10: Add the MCP server</a></dd>');
    const closed = show({ panelOpen: false });
    expect(closed).toContain('aria-expanded="false"');
    expect(slice(closed, "<dl", ">")).toContain("hidden");
  });
});
