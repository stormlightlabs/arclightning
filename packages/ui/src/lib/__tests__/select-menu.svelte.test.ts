import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import SelectMenuHarness from "./SelectMenuHarness.svelte";

const combobox = () => page.getByRole("combobox", { name: "Status", exact: true });
const value = () => page.getByTestId("value");

describe("SelectMenu", () => {
  it("is named by its label alone and shows the chosen option's label", async () => {
    await render(SelectMenuHarness);
    await expect.element(combobox()).toHaveAccessibleName("Status");
    await expect.element(combobox()).toHaveTextContent("Any");
    await expect.element(combobox()).toHaveAttribute("aria-expanded", "false");
  });

  it("opens with ArrowDown, skips disabled options and chooses with Enter", async () => {
    await render(SelectMenuHarness);
    combobox().element().focus();
    await userEvent.keyboard("{ArrowDown}");
    await expect.element(page.getByRole("listbox", { name: "Status" })).toBeVisible();
    await expect
      .element(combobox())
      .toHaveAttribute("aria-activedescendant", page.getByRole("option", { name: "Any" }).element().id);
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    await expect.element(page.getByRole("option", { name: "Planned" })).toHaveAttribute("data-active", "true");
    await userEvent.keyboard("{Enter}");
    await expect.element(value()).toHaveTextContent("planned");
    await expect.element(page.getByRole("listbox")).not.toBeInTheDocument();
    await expect.element(combobox()).toHaveFocus();
    await expect.element(combobox()).toHaveTextContent("Planned");
  });

  it("jumps with Home and End and chooses with Space", async () => {
    await render(SelectMenuHarness);
    combobox().element().focus();
    await userEvent.keyboard("{End}");
    await expect.element(page.getByRole("option", { name: "Completed" })).toHaveAttribute("data-active", "true");
    await userEvent.keyboard("{Home}");
    await expect.element(page.getByRole("option", { name: "Any" })).toHaveAttribute("data-active", "true");
    await userEvent.keyboard("{End}[Space]");
    await expect.element(value()).toHaveTextContent("completed");
  });

  it("moves to the option typed, cycling on a repeated letter", async () => {
    await render(SelectMenuHarness);
    combobox().element().focus();
    await userEvent.keyboard("p");
    await expect.element(page.getByRole("option", { name: "Pending" })).toHaveAttribute("data-active", "true");
    await userEvent.keyboard("p");
    await expect.element(page.getByRole("option", { name: "Planned" })).toHaveAttribute("data-active", "true");
  });

  it("closes on Escape without choosing and keeps focus on the combobox", async () => {
    await render(SelectMenuHarness);
    combobox().element().focus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{Escape}");
    await expect.element(page.getByRole("listbox")).not.toBeInTheDocument();
    await expect.element(combobox()).toHaveFocus();
    await expect.element(value()).toHaveTextContent("");
    await expect.element(page.getByTestId("changes")).toHaveTextContent("");
  });

  it("chooses an option by click and closes on a click outside", async () => {
    await render(SelectMenuHarness);
    await combobox().click();
    await page.getByRole("option", { name: "Pending" }).click();
    await expect.element(value()).toHaveTextContent("pending");
    await expect.element(combobox()).toHaveFocus();
    await combobox().click();
    await expect.element(page.getByRole("listbox")).toBeVisible();
    await page.getByRole("button", { name: "Outside" }).click();
    await expect.element(page.getByRole("listbox")).not.toBeInTheDocument();
  });

  it("ignores a disabled option and does not report an unchanged value", async () => {
    await render(SelectMenuHarness);
    await combobox().click();
    await page.getByRole("option", { name: "Parked" }).click({ force: true });
    await expect.element(page.getByRole("listbox")).toBeVisible();
    await page.getByRole("option", { name: "Any" }).click();
    await expect.element(page.getByTestId("changes")).toHaveTextContent("");
  });

  it("keeps the listbox inside the viewport when the trigger sits at the right edge", async () => {
    await render(SelectMenuHarness, { end: true });
    await combobox().click();
    const box = page.getByRole("listbox").element().getBoundingClientRect();
    expect(box.right).toBeLessThanOrEqual(document.documentElement.clientWidth);
  });

  it("shows a visible focus ring in the accent color in both themes", async () => {
    for (const [theme, color] of [
      ["light", "rgb(37, 99, 235)"],
      ["dark", "rgb(76, 141, 255)"],
    ] as const) {
      const screen = await render(SelectMenuHarness, { theme });
      combobox().element().focus();
      await userEvent.keyboard("{Escape}");
      const style = getComputedStyle(combobox().element());
      expect(style.outlineStyle).toBe("solid");
      expect(style.outlineColor).toBe(color);
      screen.unmount();
    }
  });
});

describe("Select", () => {
  it("keeps the options out of the select's accessible name", async () => {
    await render(SelectMenuHarness);
    await expect.element(page.getByRole("combobox", { name: "Priority" })).toHaveAccessibleName("Priority");
  });
});
