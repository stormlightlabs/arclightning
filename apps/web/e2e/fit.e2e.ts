import { expect, test } from "@playwright/test";
import api from "./fixtures.json" with { type: "json" };
import { VIEWS, open, serveFixtures } from "./fixtures";

test.describe("a 390 px window", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  for (const view of VIEWS) {
    test(`fits ${view.name} without scrolling sideways`, async ({ page }) => {
      await serveFixtures(page);
      await open(page, view);
      const { scrollWidth, clientWidth } = await page.evaluate(() => {
        const { scrollWidth, clientWidth } = document.documentElement;
        return { scrollWidth, clientWidth };
      });
      expect(scrollWidth).toBeLessThanOrEqual(clientWidth);
    });
  }

  test("hides the views behind a Menu button that the keyboard opens", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[1]);
    const views = page.getByRole("navigation", { name: "Views" });
    const menu = views.getByRole("button", { name: "Menu" });
    await expect(views.getByRole("link", { name: "Inbox" })).toBeHidden();

    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await expect(menu).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(menu).toHaveAttribute("aria-expanded", "true");
    const inbox = views.getByRole("link", { name: "Inbox" });
    await inbox.focus();
    await page.keyboard.press("Enter");
    await expect(page.getByRole("heading", { level: 1, name: "Inbox" })).toBeVisible();
    // Choosing a view closes the menu.
    await expect(menu).toHaveAttribute("aria-expanded", "false");
    await expect(inbox).toBeHidden();
  });

  test("Escape or a click outside closes the Menu and keeps the page", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[3]);
    const views = page.getByRole("navigation", { name: "Views" });
    const menu = views.getByRole("button", { name: "Menu" });
    const inbox = views.getByRole("link", { name: "Inbox" });

    await menu.click();
    await inbox.focus();
    await page.keyboard.press("Escape");
    await expect(menu).toHaveAttribute("aria-expanded", "false");
    await expect(menu).toBeFocused();
    // The Task view's Escape, which goes back to Work, did not run.
    await expect(page.getByRole("heading", { level: 1, name: VIEWS[3].heading })).toBeVisible();

    await menu.click();
    await expect(inbox).toBeVisible();
    await page.getByRole("main").click({ position: { x: 4, y: 4 } });
    await expect(menu).toHaveAttribute("aria-expanded", "false");
    await expect(inbox).toBeHidden();
  });
});

test.describe("a 390 by 260 px window", () => {
  test.use({ viewport: { width: 390, height: 260 } });

  test("keeps the row the keyboard selects below the top bar", async ({ page }) => {
    await serveFixtures(page);
    // Thirty tasks, so the list scrolls well past the window.
    const work = structuredClone(api["/api/work"]);
    const [first] = work.data.tasks;
    work.data.tasks = Array.from({ length: 30 }, (_, index) => ({
      ...first,
      task: { ...first.task, id: `arcl-t-${100 + index}`, title: `Task ${index + 1}`, position: index },
    }));
    await page.route("**/api/work", (route) => route.fulfill({ contentType: "application/json", body: JSON.stringify(work) }));
    await open(page, VIEWS[1]);

    for (let step = 0; step < 10; step++) await page.keyboard.press("j");
    // Scroll the selected row up to the bar, so the row above it sits under
    // the bar but inside the window, then select that row.
    await page.evaluate(() => {
      const bar = document.querySelector(".sidebar")!.getBoundingClientRect();
      window.scrollBy(0, document.activeElement!.closest("li")!.getBoundingClientRect().top - bar.bottom);
    });
    await page.keyboard.press("k");
    const row = page.locator(":focus");
    await expect(row).toHaveText("Task 9");
    const bar = await page.locator(".sidebar").boundingBox();
    const box = await row.boundingBox();
    expect(box!.y).toBeGreaterThanOrEqual(bar!.y + bar!.height);
    expect(box!.y + box!.height).toBeLessThanOrEqual(260);
  });
});

test.describe("themes and motion", () => {
  for (const colorScheme of ["light", "dark"] as const) {
    test(`the ${colorScheme} system setting picks the ${colorScheme} theme`, async ({ page }) => {
      await page.emulateMedia({ colorScheme });
      await serveFixtures(page);
      await open(page, VIEWS[1]);
      const scheme = await page.evaluate(() => getComputedStyle(document.documentElement).colorScheme);
      expect(scheme).toBe(colorScheme);
      const canvas = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
      expect(canvas).toBe(colorScheme === "light" ? "rgb(244, 245, 247)" : "rgb(9, 9, 11)");
    });
  }

  /** The longest animation or transition any element on the page runs, in ms. */
  const longestMotion = () =>
    [...document.querySelectorAll("*")].reduce((longest, element) => {
      const style = getComputedStyle(element);
      const durations = [
        ...(style.animationName === "none" ? [] : style.animationDuration.split(",")),
        ...style.transitionDuration.split(","),
      ].map((value) => parseFloat(value) * (value.trim().endsWith("ms") ? 1 : 1000));
      return Math.max(longest, ...durations);
    }, 0);

  for (const [reducedMotion, moves] of [
    ["no-preference", true],
    ["reduce", false],
  ] as const) {
    test(`motion ${moves ? "runs" : "stops"} when reduced motion is ${reducedMotion}`, async ({ page }) => {
      await page.emulateMedia({ reducedMotion });
      await serveFixtures(page);
      await open(page, VIEWS[3]);
      // The dialog and the properties chevron animate unless motion is reduced.
      await page.keyboard.press("q");
      await expect(page.getByRole("dialog", { name: "Quick capture" })).toBeVisible();
      const longest = await page.evaluate(longestMotion);
      if (moves) expect(longest).toBeGreaterThan(50);
      else expect(longest).toBeLessThanOrEqual(0.01);
    });
  }
});
