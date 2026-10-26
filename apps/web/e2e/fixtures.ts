import type { Page } from "@playwright/test";
import api from "./fixtures.json" with { type: "json" };

/**
 * The views the fit and screenshot tests open, with the heading each shows.
 * `fixtures.json` holds what `arcl serve` answered for a scratch project, with
 * IDs and timestamps replaced by fixed ones, so every run draws the same page.
 */
export const VIEWS = [
  { name: "inbox", path: "/inbox", heading: "Inbox" },
  { name: "work", path: "/", heading: "Work" },
  { name: "planning", path: "/planning", heading: "Planning" },
  { name: "task", path: "/tasks/arcl-t-2", heading: "Store pending writes in SQLite" },
  { name: "board", path: "/?view=board", heading: "Work" },
] as const;

/** Answer the API's reads from `fixtures.json` and fail on any page error. */
export async function serveFixtures(page: Page) {
  page.on("pageerror", (error) => {
    throw error;
  });
  await page.route("**/api/**", async (route) => {
    const url = new URL(route.request().url());
    const body = (api as Record<string, unknown>)[url.pathname + url.search];
    if (route.request().method() !== "GET" || body === undefined) return route.fulfill({ status: 404, body: "{}" });
    return route.fulfill({ contentType: "application/json", body: JSON.stringify(body) });
  });
}

/** Open `view` and wait until its data and fonts are in. */
export async function open(page: Page, view: (typeof VIEWS)[number]) {
  await page.goto(view.path);
  await page.getByRole("heading", { level: 1, name: view.heading }).waitFor();
  await page
    .getByRole("status")
    .filter({ hasText: /^Loading/ })
    .waitFor({ state: "detached" });
  await page.evaluate(() => document.fonts.ready);
}
