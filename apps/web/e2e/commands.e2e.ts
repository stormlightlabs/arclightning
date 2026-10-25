import { expect, test, type Page } from "@playwright/test";

/** A pending, ready task as the CLI's JSON names it. */
const task = {
  id: "arcl-t-1",
  spec_id: null,
  plan_id: null,
  phase_id: null,
  parent_id: null,
  title: "T1: Add the skill",
  body: "",
  status: "pending",
  priority: "normal",
  position: 0,
  handoff: "",
  evidence: "",
};
const readiness = { ready: true, blocked: false, reasons: [] };
const detail = { task, readiness, blockers: [], inherited_blockers: [], dependents: [], children: [], ancestors: [], spec: null, plan: null, phase: null };
const envelope = (data: unknown) => JSON.stringify({ format_version: 1, data });

/** Answer the API from fixtures and record each POST as `path body`. */
async function serve(page: Page, status = "pending"): Promise<string[]> {
  const shown = { ...task, status };
  const posts: string[] = [];
  page.on("pageerror", (error) => {
    throw error;
  });
  await page.route("**/api/**", async (route) => {
    const request = route.request();
    const url = new URL(request.url());
    const path = url.pathname + url.search;
    if (request.method() === "POST") {
      posts.push(`${url.pathname} ${request.postData() ?? ""}`);
      const body = JSON.parse(request.postData() ?? "{}");
      if (url.pathname === "/api/captures") {
        return route.fulfill({ body: envelope({ action: "created", capture: { id: "arcl-c-1", title: body.title, body: body.body ?? "", status: "captured", created_at: "" } }) });
      }
      if (url.pathname === "/api/tasks") {
        return route.fulfill({ body: envelope({ action: "created", task: { ...task, id: "arcl-t-2", title: body.title } }) });
      }
      return route.fulfill({ body: envelope({ action: "started", task: { ...task, status: "in_progress" } }) });
    }
    if (path === "/api/records?kind=task") return route.fulfill({ body: envelope({ records: [{ id: task.id, kind: "task", status, title: task.title }] }) });
    if (path === "/api/records?kind=phase") return route.fulfill({ body: envelope({ records: [] }) });
    if (path === `/api/tasks/${task.id}/explain`) return route.fulfill({ body: envelope({ task: shown, readiness, phase: null }) });
    if (path.startsWith("/api/records/arcl-t-")) return route.fulfill({ body: envelope({ ...detail, task: shown }) });
    return route.fulfill({ status: 404, body: "{}" });
  });
  return posts;
}

test("the menu lists the open task's actions first and runs the same change as the button", async ({ page }) => {
  const posts = await serve(page);
  await page.goto(`/tasks/${task.id}`);
  await expect(page.getByRole("heading", { level: 1, name: task.title })).toBeVisible();

  await page.getByRole("button", { name: "Start" }).click();
  await expect.poll(() => posts.length).toBe(1);

  await page.keyboard.press("Control+k");
  const menu = page.getByRole("dialog", { name: "Command menu" });
  await expect(menu.getByRole("combobox")).toBeFocused();
  const groups = menu.getByRole("group");
  await expect(groups.first()).toHaveAccessibleName(`Actions on ${task.title}`);
  await expect(groups.first().getByRole("option").first()).toHaveText(/Start\s*S/);
  await page.keyboard.press("Enter");
  await expect(menu).toBeHidden();
  await expect.poll(() => posts.length).toBe(2);
  expect(posts[1]).toBe(posts[0]);

  // The shortcut waits, like the button, until the change and its reload finish.
  await expect(page.getByRole("button", { name: "Start" })).toBeEnabled();
  await page.keyboard.press("s");
  await expect.poll(() => posts.length).toBe(3);
  expect(posts[2]).toBe(posts[0]);
});

test("the menu acts on the Work list's selected row", async ({ page }) => {
  const posts = await serve(page);
  await page.goto("/");
  await expect(page.getByRole("link", { name: new RegExp(task.title) })).toBeVisible();
  await page.keyboard.press("j");
  await page.keyboard.press("Control+k");
  await page.keyboard.type("start");
  await page.keyboard.press("Enter");
  await expect.poll(() => posts).toEqual([`/api/tasks/${task.id}/start {}`]);
});

test("from the Work list, an action that takes text opens the Task view with its form", async ({ page }) => {
  const posts = await serve(page, "in_progress");
  await page.goto("/");
  await expect(page.getByRole("link", { name: new RegExp(task.title) })).toBeVisible();
  await page.keyboard.press("j");
  await page.keyboard.press("h");
  await expect(page).toHaveURL(new RegExp(`/tasks/${task.id}$`));
  await expect(page.getByRole("form", { name: "Hand off" })).toBeVisible();
  expect(posts).toEqual([]);
});

test("q opens quick capture from any view, and its tokens set the task's fields", async ({ page }) => {
  const posts = await serve(page);
  for (const path of ["/", "/inbox", "/planning", `/tasks/${task.id}`]) {
    await page.goto(path);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.keyboard.press("q");
    await expect(page.getByRole("dialog", { name: "Quick capture" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("dialog")).toHaveCount(0);
  }

  await page.keyboard.press("q");
  await page.keyboard.type("An idea !high");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("status")).toHaveText("Captured “An idea !high” in the Inbox.");

  await page.keyboard.press("q");
  await page.keyboard.type("Follow up !high after:T1");
  await page.keyboard.press("Control+Enter");
  await expect(page).toHaveURL(/\/tasks\/arcl-t-2$/);
  expect(posts).toEqual([
    '/api/captures {"title":"An idea !high"}',
    `/api/tasks {"title":"Follow up","priority":"high","blocked_by":["${task.id}"]}`,
  ]);
});

test("shortcuts do not fire while typing in a field", async ({ page }) => {
  await serve(page);
  await page.goto("/");
  await page.getByRole("button", { name: "New task" }).click();
  await page.getByLabel("Title").fill("");
  await page.getByLabel("Title").press("q");
  await page.getByLabel("Title").press("Control+k");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(page.getByLabel("Title")).toHaveValue("q");
});
