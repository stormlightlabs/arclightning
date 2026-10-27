import { expect, test, type Page } from "@playwright/test";
import { serveScratch, stopAll, type Served } from "./serve";

let served: Served | undefined;

// The page reads the clipboard back to check what a copy wrote.
test.use({ permissions: ["clipboard-read", "clipboard-write"] });

test.beforeEach(async ({ page }) => {
  served = undefined;
  served = await serveScratch();
  page.on("pageerror", (error) => {
    throw error;
  });
});

// `stopAll` also cleans up a `serveScratch` that timed out before `served` was set.
test.afterEach(async () => {
  await stopAll();
});
test.afterAll(async () => {
  await stopAll();
});

/** What the clipboard holds now. */
function clipboard(page: Page): Promise<string> {
  return page.evaluate(() => navigator.clipboard.readText());
}

test("the Task view's copy button puts the full ID on the clipboard", async ({ page }) => {
  const id: string = served!.cli("task", "create", "Copy task").task.id;
  await page.goto(served!.url);
  await page.getByRole("link", { name: "Copy task" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Copy task" })).toBeVisible();

  const header = page.getByRole("article").locator("header");
  await header.getByRole("button", { name: "Copy ID" }).click();
  await expect(header.getByRole("status").filter({ hasText: "Copied" })).toBeVisible();
  expect(await clipboard(page)).toBe(id);
});

// Nothing on a card reacts to a click, so the copy-id component test covers stopping its propagation.
test("clicking a board card's ID copies it", async ({ page }) => {
  const id: string = served!.cli("task", "create", "Board copy").task.id;
  await page.goto(served!.url);
  await page.getByRole("button", { name: "Board" }).click();
  const card = page.getByRole("region", { name: "Board" }).getByRole("listitem").filter({ hasText: "Board copy" });
  await card.getByText(id, { exact: true }).click();
  await expect(card.getByRole("status")).toHaveText("Copied");
  expect(await clipboard(page)).toBe(id);
});
