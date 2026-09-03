import { expect, test } from "@playwright/test";

test("shared component states remain stable", async ({ page }, testInfo) => {
  const story = testInfo.project.name === "dark" ? "dark" : "light";
  await page.goto(`/iframe.html?id=review-all-states--${story}&viewMode=story`);
  await page.evaluate(() => document.fonts.ready);
  await expect(page.getByRole("heading", { name: "Shared component review" })).toBeVisible();
  await expect(page).toHaveScreenshot(`shared-components-${story}.png`, { fullPage: true });
});
