import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import DataTableHarness from "./DataTableHarness.svelte";

const items = Array.from({ length: 12 }, (_, index) => ({
  id: String.fromCharCode(97 + index),
  title: `Item ${index + 1}`,
  note: `Note ${index + 1}`,
}));

describe("DataTable", () => {
  it("shows a header and one row per item, with column headers", async () => {
    await render(DataTableHarness, { items });
    const table = page.getByRole("table");
    await expect.element(table.getByRole("columnheader", { name: "Title" })).toBeVisible();
    await expect.element(table.getByRole("columnheader", { name: "Note" })).toBeVisible();
    expect(table.getByRole("row").elements()).toHaveLength(13);
    await expect.element(table.getByRole("cell", { name: "Note 3" })).toBeInTheDocument();
  });

  it("selects a row on click anywhere in it, and moves focus with the selection", async () => {
    await render(DataTableHarness, { items });
    await page.getByRole("cell", { name: "Note 3" }).click();
    await expect.element(page.getByRole("status")).toHaveTextContent("c");
    const third = page.getByRole("button", { name: "Item 3" });
    await expect.element(third).toHaveAttribute("aria-current", "true");
    await expect.element(third).toHaveFocus();
    await page.getByRole("button", { name: "Select second" }).click();
    // Focus left the table for the outside button, so it stays there.
    await expect.element(page.getByRole("button", { name: "Item 2" })).toHaveAttribute("aria-current", "true");
    await expect.element(page.getByRole("button", { name: "Select second" })).toHaveFocus();
  });

  it("selects rows with the arrow keys, Home and End, from the focused row", async () => {
    await render(DataTableHarness, { items });
    await page.getByRole("button", { name: "Item 1", exact: true }).element().focus();
    await userEvent.keyboard("{ArrowDown}");
    await expect.element(page.getByRole("status")).toHaveTextContent("b");
    await expect.element(page.getByRole("button", { name: "Item 2" })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}{ArrowUp}");
    await expect.element(page.getByRole("status")).toHaveTextContent("a");
    await userEvent.keyboard("{End}");
    await expect.element(page.getByRole("status")).toHaveTextContent("l");
    await expect.element(page.getByRole("button", { name: "Item 12" })).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}{Home}");
    await expect.element(page.getByRole("status")).toHaveTextContent("a");
    await expect.element(page.getByRole("button", { name: "Item 1", exact: true })).toHaveFocus();
  });

  it("reaches only the selected row, or the first, with Tab", async () => {
    await render(DataTableHarness, { items });
    const tabbable = () =>
      page
        .getByRole("button", { name: /^Item/ })
        .elements()
        .filter((button) => button.tabIndex === 0);
    expect(tabbable().map((button) => button.textContent?.trim())).toEqual(["Item 1"]);
    await page.getByRole("button", { name: "Select second" }).click();
    expect(tabbable().map((button) => button.textContent?.trim())).toEqual(["Item 2"]);
  });

  it("scrolls inside its box under a sticky header, without overflowing sideways", async () => {
    await render(DataTableHarness, { items });
    const region = page.getByRole("region", { name: "Items" }).element() as HTMLElement;
    const box = region.parentElement!.getBoundingClientRect();
    const bounds = region.getBoundingClientRect();
    expect(bounds.right).toBeLessThanOrEqual(box.right);
    expect(bounds.bottom).toBeLessThanOrEqual(box.bottom);
    expect(region.scrollHeight).toBeGreaterThan(region.clientHeight);
    expect(region.scrollWidth).toBeLessThanOrEqual(region.clientWidth);
    region.scrollTop = region.scrollHeight;
    const header = page.getByRole("columnheader", { name: "Title" }).element().getBoundingClientRect();
    expect(Math.round(header.top)).toBe(Math.round(bounds.top));
  });

  it("hides narrow-only columns when the table is narrow", async () => {
    await render(DataTableHarness, { items, width: "20rem" });
    await expect.element(page.getByRole("columnheader", { name: "Title" })).toBeVisible();
    // A hidden column leaves the accessibility tree, so look for its cells directly.
    const hidden = [...document.querySelectorAll("th, td")].filter((cell) => /^Note/.test(cell.textContent ?? ""));
    expect(hidden).toHaveLength(13);
    expect(hidden.every((cell) => getComputedStyle(cell).display === "none")).toBe(true);
  });
});
