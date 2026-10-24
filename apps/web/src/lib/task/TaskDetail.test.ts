import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import { bareDetail, blockedDetail } from "./fixtures";
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
  });

  it("renders the body as Markdown with a way to open its source", () => {
    const description = slice(show(), 'id="task-description"', "</section>");
    expect(description).toContain("<li>[ ] A run passes through MCP tools only</li>");
    expect(description).toContain('aria-pressed="false"');
    expect(description).toContain("View Markdown");
  });

  it("links the breadcrumb back to the Work list and lists the unblocked task", () => {
    const body = show();
    expect(slice(body, 'aria-label="Breadcrumb"', "</nav>")).toContain('href="/?selected=t-10.4"');
    expect(slice(body, 'id="task-unblocks"', "</section>")).toContain('href="/tasks/t-10.5"');
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
