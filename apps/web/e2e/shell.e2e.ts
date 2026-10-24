import { expect, test } from "@playwright/test";

test.beforeEach(({ page }) => {
  page.on("pageerror", (error) => {
    throw error;
  });
});

test("the shell drops the token from the URL and keeps it for the tab", async ({ page }) => {
  await page.goto("/?token=abc");
  await expect(page).toHaveURL(/\/$/);
  expect(await page.evaluate(() => sessionStorage.getItem("arcl-token"))).toBe("abc");

  await page.getByRole("link", { name: "Inbox" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Inbox" })).toBeVisible();
});

test("the shell navigates between Inbox, Work and Planning", async ({ page }) => {
  await page.goto("/");
  const views = page.getByRole("navigation", { name: "Views" });
  await expect(views.getByRole("link", { name: "Work" })).toHaveAttribute("aria-current", "page");

  for (const name of ["Inbox", "Planning", "Work"]) {
    await views.getByRole("link", { name }).click();
    await expect(page.getByRole("heading", { level: 1, name })).toBeVisible();
    await expect(views.getByRole("link", { name })).toHaveAttribute("aria-current", "page");
  }
});
