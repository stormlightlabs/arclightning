import { createRawSnippet } from "svelte";
import { page as screen, userEvent } from "vitest/browser";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "vitest-browser-svelte";
import { SIDEBAR_STORAGE_KEY } from "$lib/sidebar";
import Layout from "../../routes/+layout.svelte";
import { reset } from "./app.svelte";

vi.mock("$app/state", async () => ({ page: (await import("./app.svelte")).page }));
vi.mock("$app/navigation", async () => {
  const app = await import("./app.svelte");
  return { afterNavigate: app.afterNavigate, goto: app.goto };
});

const children = createRawSnippet(() => ({ render: () => "<h1>View</h1>" }));
const sidebar = () => screen.getByRole("navigation", { name: "Views" });

beforeEach(() => {
  reset();
  window.localStorage.removeItem(SIDEBAR_STORAGE_KEY);
});
afterEach(async () => {
  window.localStorage.removeItem(SIDEBAR_STORAGE_KEY);
  await screen.viewport(1024, 768);
});

describe("the sidebar", () => {
  it("collapses to a rail of icons whose links keep their names, and remembers it", async () => {
    const first = await render(Layout, { children });
    const full = sidebar().element().getBoundingClientRect().width;
    await sidebar().getByRole("button", { name: "Collapse sidebar" }).click();
    expect(window.localStorage.getItem(SIDEBAR_STORAGE_KEY)).toBe("collapsed");
    expect(sidebar().element().getBoundingClientRect().width).toBeLessThan(full / 3);
    await expect.element(sidebar().getByRole("link", { name: "Inbox" })).toBeVisible();
    await expect.element(sidebar().getByRole("button", { name: "Search" })).toBeVisible();
    await expect.element(sidebar().getByText("Arc Lightning")).not.toBeVisible();

    first.unmount();
    await render(Layout, { children });
    await expect.element(sidebar().getByRole("button", { name: "Expand sidebar" })).toBeVisible();
    expect(sidebar().element().getBoundingClientRect().width).toBeLessThan(full / 3);
  });

  it("toggles with [ and keeps the choice", async () => {
    // `[[` types one `[` in userEvent's key syntax.
    await render(Layout, { children });
    await userEvent.keyboard("[[");
    await expect.element(sidebar().getByRole("button", { name: "Expand sidebar" })).toBeVisible();
    expect(window.localStorage.getItem(SIDEBAR_STORAGE_KEY)).toBe("collapsed");
    await userEvent.keyboard("[[");
    await expect.element(sidebar().getByRole("button", { name: "Collapse sidebar" })).toBeVisible();
    expect(window.localStorage.getItem(SIDEBAR_STORAGE_KEY)).toBe("expanded");
  });

  it("gives way to the top bar and its Menu in a narrow window, even when collapsed", async () => {
    window.localStorage.setItem(SIDEBAR_STORAGE_KEY, "collapsed");
    await screen.viewport(390, 844);
    await render(Layout, { children });
    await expect.element(sidebar().getByRole("button", { name: "Menu" })).toBeVisible();
    // A hidden button leaves the accessibility tree, so find it by its class.
    expect(getComputedStyle(sidebar().element().querySelector(".collapse-toggle")!).display).toBe("none");
    await expect.element(sidebar().getByText("Arc Lightning")).toBeVisible();
    expect(Math.round(sidebar().element().getBoundingClientRect().width)).toBe(390);
  });

  it("leaves [ alone in a narrow window, which has no sidebar to collapse", async () => {
    await screen.viewport(390, 844);
    await render(Layout, { children });
    await expect.element(sidebar().getByRole("button", { name: "Menu" })).toBeVisible();
    await userEvent.keyboard("[[");
    expect(window.localStorage.getItem(SIDEBAR_STORAGE_KEY)).toBeNull();
    await screen.viewport(1024, 768);
    await expect.element(sidebar().getByRole("button", { name: "Collapse sidebar" })).toBeVisible();
  });
});
