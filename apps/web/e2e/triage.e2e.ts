import { expect, test, type Page } from "@playwright/test";

const envelope = (data: unknown) => JSON.stringify({ format_version: 1, data });
const captures = [
  { id: "arcl-c-1", kind: "capture", status: "captured", title: "Fix the bug !high" },
  { id: "arcl-c-2", kind: "capture", status: "captured", title: "Old idea" },
];
const tree = [
  {
    kind: "spec",
    id: "arcl-s-1",
    title: "Spec",
    status: "open",
    children: [{ kind: "plan", id: "arcl-pl-1", title: "Plan", status: "open", children: [] }],
  },
];

/** Answer the API from fixtures and record each POST as `path body`. */
async function serve(page: Page): Promise<string[]> {
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
      if (url.pathname.endsWith("/promote")) {
        return route.fulfill({
          body: envelope({ capture: {}, promotion: {}, [body.to]: { id: `arcl-${body.to}-1`, title: body.title } }),
        });
      }
      if (url.pathname.endsWith("/discard"))
        return route.fulfill({ body: envelope({ action: "discarded", capture: {} }) });
      return route.fulfill({
        body: envelope({
          action: "created",
          plan: { id: "arcl-pl-2", spec_id: "arcl-s-1", title: body.title, body: "", status: "open" },
        }),
      });
    }
    if (path === "/api/records?kind=capture&status=captured")
      return route.fulfill({ body: envelope({ records: captures }) });
    if (path.startsWith("/api/records/arcl-c-")) {
      const id = url.pathname.split("/").at(-1);
      const capture = captures.find((entry) => entry.id === id);
      return route.fulfill({
        body: envelope({ action: "shown", capture: { ...capture, body: "", created_at: "2026-10-24T00:00:00Z" } }),
      });
    }
    if (path === "/api/work") return route.fulfill({ body: envelope({ tasks: [], phases: [] }) });
    if (path === "/api/tree") return route.fulfill({ body: envelope({ nodes: tree }) });
    return route.fulfill({ status: 404, body: "{}" });
  });
  return posts;
}

test("the Inbox triages the selected capture with 1 and Backspace", async ({ page }) => {
  const posts = await serve(page);
  await page.goto("/inbox");
  await expect(page.getByRole("heading", { level: 2, name: "Fix the bug !high" })).toBeVisible();

  await page.keyboard.press("1");
  const title = page.getByRole("textbox", { name: "Title" });
  await expect(title).toBeFocused();
  // Keys typed into the form stay text.
  await page.keyboard.type(" 2");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("link", { name: "Open task" })).toHaveAttribute("href", "/tasks/arcl-task-1");
  await expect(page.getByRole("heading", { level: 2, name: "Old idea" })).toBeVisible();

  await page.keyboard.press("Backspace");
  await expect(page.getByRole("dialog", { name: "Discard this capture?" })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByText("The Inbox is empty")).toBeVisible();
  expect(posts).toEqual([
    '/api/captures/arcl-c-1/promote {"to":"task","title":"Fix the bug 2","priority":"high"}',
    "/api/captures/arcl-c-2/discard {}",
  ]);
});

test("Planning adds a plan to the selected spec with a", async ({ page }) => {
  const posts = await serve(page);
  await page.goto("/planning");
  await expect(page.getByRole("button", { name: "Spec Spec · 1 plan" })).toBeVisible();
  await page.keyboard.press("j");
  await page.keyboard.press("a");
  await expect(page.getByRole("textbox", { name: "Title" })).toBeFocused();
  await page.keyboard.type("Second plan");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("status")).toContainText("Created the plan “Second plan”.");
  expect(posts).toEqual(['/api/plans {"title":"Second plan","spec":"arcl-s-1"}']);
});
