import { test } from "@playwright/test";
import { fileURLToPath } from "node:url";
import { VIEWS, open, serveFixtures } from "./fixtures";

/** Where the README's images live. */
const IMAGES = fileURLToPath(new URL("../../../docs/images/", import.meta.url));

/** The corner radius of each image, in CSS pixels: 24 image pixels at twice the density. */
const RADIUS = 12;

/**
 * The README's screenshots: dark, desktop-sized and drawn from the same
 * fixtures as the visual tests at twice the pixel density, with rounded
 * corners on a transparent background. `pnpm --filter @arclightning/web
 * screenshots` rewrites them in `docs/images/`.
 */
const SHOTS = [
  { file: "work-board-dark.png", view: VIEWS[4] },
  { file: "task-dark.png", view: VIEWS[3] },
] as const;

for (const shot of SHOTS) {
  test.describe(shot.file, () => {
    test.use({ viewport: { width: 1280, height: 800 }, colorScheme: "dark", deviceScaleFactor: 2 });

    test(shot.file, async ({ page, context }) => {
      await serveFixtures(page);
      await open(page, shot.view);
      const image = await page.screenshot({ animations: "disabled", caret: "hide" });
      // Round the corners in the browser, and keep what lies outside them
      // transparent in the saved PNG.
      const frame = await context.newPage();
      await frame.setContent(
        `<body style="margin:0;background:transparent"><img alt="" style="display:block;width:1280px;height:800px;border-radius:${RADIUS}px" src="data:image/png;base64,${image.toString("base64")}"></body>`,
      );
      await frame.locator("img").screenshot({ path: IMAGES + shot.file, omitBackground: true, animations: "disabled" });
    });
  });
}
