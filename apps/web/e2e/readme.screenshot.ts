import { test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { VIEWS, open, serveFixtures } from "./fixtures";

/** Where the README's images live. */
const IMAGES = fileURLToPath(new URL("../../../docs/images/", import.meta.url));

/**
 * The README's screenshots, drawn from the same fixtures as the visual tests at
 * twice the pixel density. `pnpm --filter @arclightning/web screenshots`
 * rewrites them in `docs/images/`.
 */
const SHOTS = [
  { file: "task-light.png", view: VIEWS[3], viewport: { width: 1280, height: 800 }, colorScheme: "light" },
  { file: "work-narrow-dark.png", view: VIEWS[1], viewport: { width: 390, height: 844 }, colorScheme: "dark" },
] as const;

for (const shot of SHOTS) {
  test.describe(shot.file, () => {
    test.use({ viewport: shot.viewport, colorScheme: shot.colorScheme, deviceScaleFactor: 2 });

    test(shot.file, async ({ page }) => {
      await serveFixtures(page);
      await open(page, shot.view);
      await page.screenshot({ path: IMAGES + shot.file, fullPage: true, animations: "disabled", caret: "hide" });
    });
  });
}
