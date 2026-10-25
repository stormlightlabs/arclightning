import { expect, test, type Page } from "@playwright/test";
import { serveScratch, type Served } from "./serve";

/** A record as the CLI's JSON lists it. */
interface Listed {
  id: string;
  title: string;
  status: string;
  [field: string]: unknown;
}

let served: Served;

test.beforeEach(async ({ page }) => {
  served = await serveScratch();
  page.on("pageerror", (error) => {
    throw error;
  });
});

test.afterEach(async () => {
  await served.stop();
});

/** The records of `kind` the CLI lists, such as `task` or `spec`. */
function listed(kind: string): Listed[] {
  return served.cli(kind, "list")[`${kind}s`];
}

/** Press each key in turn, like `g` then `i`. */
async function press(page: Page, ...keys: string[]) {
  for (const key of keys) await page.keyboard.press(key);
}

/** The status message that says `text`. */
function status(page: Page, text: string) {
  return page.getByRole("status").filter({ hasText: text });
}

/** File a capture with quick capture, from whichever view is open. */
async function capture(page: Page, title: string) {
  await page.keyboard.press("q");
  await expect(page.getByRole("dialog", { name: "Quick capture" }).getByRole("textbox", { name: "Capture" })).toBeFocused();
  await page.keyboard.type(title);
  await page.keyboard.press("Enter");
  await expect(status(page, `Captured “${title}” in the Inbox.`)).toBeVisible();
}

test("keyboard alone takes captures to tasks through the Inbox and Planning, then starts and completes one", async ({ page }) => {
  await page.goto(served.url);
  await expect(page.getByRole("heading", { level: 1, name: "Work" })).toBeVisible();

  // Capture -> task.
  await capture(page, "Write the changelog");
  await press(page, "g", "i");
  await expect(page.getByRole("heading", { level: 2, name: "Write the changelog" })).toBeVisible();
  await page.keyboard.press("1");
  await expect(page.getByRole("textbox", { name: "Title" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("link", { name: "Open task" })).toBeVisible();
  expect(listed("task").map((task) => task.title)).toEqual(["Write the changelog"]);
  expect(listed("capture").find((entry) => entry.title === "Write the changelog")?.status).toBe("promoted");

  // Capture -> spec -> plan -> task. The open Inbox does not list a capture
  // filed from quick capture until it loads again, so capture from Work.
  await press(page, "g", "w");
  await expect(page.getByRole("heading", { level: 1, name: "Work" })).toBeVisible();
  await capture(page, "Offline sync");
  await press(page, "g", "i");
  await expect(page.getByRole("heading", { level: 2, name: "Offline sync" })).toBeVisible();
  await page.keyboard.press("2");
  await expect(page.getByRole("textbox", { name: "Title" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("link", { name: "Open Planning" })).toBeVisible();
  const [spec] = listed("spec");
  expect(spec.title).toBe("Offline sync");

  await press(page, "g", "p");
  await expect(page.getByRole("button", { name: "Offline sync Spec · 0 plans" })).toBeVisible();
  await press(page, "j", "a");
  await expect(page.getByRole("textbox", { name: "Title" })).toBeFocused();
  await page.keyboard.type("Queue writes");
  await page.keyboard.press("Enter");
  await expect(status(page, "Created the plan “Queue writes”.")).toBeVisible();
  const [plan] = listed("plan");
  expect(plan).toMatchObject({ title: "Queue writes", spec_id: spec.id });

  // The new plan is selected; a opens its plan document, and Tab reaches Apply.
  await page.keyboard.press("a");
  await expect(page.getByRole("textbox", { name: "Plan document" })).toBeFocused();
  await press(page, "Tab", "Enter");
  await expect(status(page, "Applied “Queue writes”: it has 1 phase and 1 task.")).toBeVisible();
  const planned = listed("task").find((task) => task.title === "First task");
  expect(planned).toMatchObject({ plan_id: plan.id, status: "pending" });

  // Start and complete, reaching the task through the command menu.
  await page.keyboard.press("Control+k");
  await page.keyboard.type("First task");
  const menu = page.getByRole("dialog", { name: "Command menu" });
  // The menu searches tasks once they load.
  await expect(menu.getByRole("option", { name: /^First task/, selected: true })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { level: 1, name: "First task" })).toBeVisible();
  await page.keyboard.press("s");
  await expect(page.getByRole("button", { name: "Complete" })).toBeVisible();
  await expect.poll(() => served.cli("task", "show", planned!.id).task.status).toBe("in_progress");

  await page.keyboard.press("c");
  const complete = page.getByRole("form", { name: "Complete" });
  await expect(complete.getByRole("textbox", { name: "Evidence Markdown" })).toBeFocused();
  await page.keyboard.type("Shipped in the scratch project.");
  await page.keyboard.press("Tab");
  await expect(complete.getByRole("button", { name: "Complete" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(complete).toBeHidden();
  const done = served.cli("task", "show", planned!.id).task;
  expect(done).toMatchObject({ status: "completed", evidence: "Shipped in the scratch project." });
});
