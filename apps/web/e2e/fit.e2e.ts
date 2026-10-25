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

  test("the Menu closes when focus moves into the page", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[1]);
    const menu = page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Menu" });
    await menu.click();
    await expect(menu).toHaveAttribute("aria-expanded", "true");
    await page.getByRole("combobox", { name: "Status" }).focus();
    await expect(menu).toHaveAttribute("aria-expanded", "false");
  });

  // A click event that moves no focus opens the Menu while focus stays in the
  // view, so these check the Escape handler apart from the focus one.
  test("Escape in the Task editor with the Menu open leaves the editor to its frame", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[3]);
    const menu = page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Menu" });
    await page.getByRole("button", { name: "Edit" }).click();
    const form = page.getByRole("form", { name: "Edit task" });
    const editor = form.getByRole("textbox", { name: "Description Markdown" });
    await editor.click();
    await expect(editor).toBeFocused();
    await menu.dispatchEvent("click");
    await expect(menu).toHaveAttribute("aria-expanded", "true");

    await page.keyboard.press("Escape");
    // CodeMirror's Escape moves focus out to the editor's frame.
    await expect(form.locator(".arcl-markdown-editor")).toBeFocused();
    await expect(menu).not.toBeFocused();
    await expect(form).toBeVisible();
  });

  test("Escape in an open Status menu with the Menu open closes the listbox only", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[1]);
    const menu = page.getByRole("navigation", { name: "Views" }).getByRole("button", { name: "Menu" });
    const status = page.getByRole("combobox", { name: "Status" });
    await status.click();
    await expect(page.getByRole("listbox", { name: "Status" })).toBeVisible();
    await menu.dispatchEvent("click");
    await expect(menu).toHaveAttribute("aria-expanded", "true");

    await page.keyboard.press("Escape");
    await expect(page.getByRole("listbox", { name: "Status" })).toBeHidden();
    await expect(status).toHaveAttribute("aria-expanded", "false");
    await expect(status).toBeFocused();
  });
});

test.describe("a 1280 px window", () => {
  test("shows the selected capture beside the Inbox's table, inside its box", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[0]);
    const table = await page.getByRole("region", { name: "Captures" }).boundingBox();
    const triage = await page.getByRole("region", { name: "Triage" }).boundingBox();
    const box = await page.locator(".columns").boundingBox();
    expect(triage!.x).toBeGreaterThanOrEqual(table!.x + table!.width - 1);
    expect(Math.abs(triage!.y - table!.y)).toBeLessThan(2);
    expect(triage!.x + triage!.width).toBeLessThanOrEqual(box!.x + box!.width);
  });

  test("[ collapses the sidebar to icons, and a reload keeps it collapsed", async ({ page }) => {
    await serveFixtures(page);
    await open(page, VIEWS[1]);
    const views = page.getByRole("navigation", { name: "Views" });
    const wide = (await views.boundingBox())!.width;
    await page.keyboard.press("[");
    await expect(views.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
    expect((await views.boundingBox())!.width).toBeLessThan(wide / 3);
    await page.reload();
    await expect(views.getByRole("button", { name: "Expand sidebar" })).toBeVisible();
    await views.getByRole("link", { name: "Inbox" }).click();
    await expect(page.getByRole("heading", { level: 1, name: "Inbox" })).toBeVisible();
    await views.getByRole("button", { name: "Expand sidebar" }).click();
    expect((await views.boundingBox())!.width).toBe(wide);
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
