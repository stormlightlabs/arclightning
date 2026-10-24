import { expect, test } from "@playwright/test";

for (const colorScheme of ["light", "dark"] as const) {
  test(`the empty shell in the ${colorScheme} theme`, async ({ page }) => {
    await page.emulateMedia({ colorScheme });
    await page.goto("/");
    await page.evaluate(() => document.fonts.ready);
    await expect(page.getByRole("heading", { level: 1, name: "Work" })).toBeVisible();
    await expect(page).toHaveScreenshot(`shell-${colorScheme}.png`, { fullPage: true });
  });
}
