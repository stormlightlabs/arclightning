import { expect, test } from "@playwright/test";
import { VIEWS, open, serveFixtures } from "./fixtures";

const WIDTHS = [
  { name: "desktop", viewport: { width: 1280, height: 800 } },
  { name: "narrow", viewport: { width: 390, height: 844 } },
] as const;

for (const width of WIDTHS) {
  for (const colorScheme of ["light", "dark"] as const) {
    test.describe(`${width.name}, ${colorScheme}`, () => {
      test.use({ viewport: width.viewport, colorScheme });

      for (const view of VIEWS) {
        test(view.name, async ({ page }) => {
          await serveFixtures(page);
          await open(page, view);
          await expect(page).toHaveScreenshot(`${view.name}-${width.name}-${colorScheme}.png`, { fullPage: true });
        });
      }
    });
  }
}
