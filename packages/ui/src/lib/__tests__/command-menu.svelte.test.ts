import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import CommandMenuHarness from "./CommandMenuHarness.svelte";

const search = () => page.getByRole("combobox", { name: "Command menu" });
const ran = () => page.getByTestId("ran");

async function openMenu() {
  await render(CommandMenuHarness);
  await page.getByRole("button", { name: "Open menu" }).click();
  await expect.element(search()).toHaveFocus();
}

describe("CommandMenu", () => {
  it("opens as a named dialog with the search focused and the first option active", async () => {
    await openMenu();
    await expect.element(page.getByRole("dialog", { name: "Command menu" })).toBeVisible();
    await expect.element(page.getByText("T10.1 · Add the skill", { exact: true })).toBeVisible();
    await expect.element(page.getByRole("group", { name: "Actions on T10.1" })).toBeVisible();
    await expect.element(search()).toHaveAttribute("aria-activedescendant", page.getByRole("option", { name: /Start/ }).element().id);
    await expect.element(page.getByRole("option", { name: /Start/ })).toHaveTextContent("S");
  });

  it("moves with the arrows and runs the active command with Enter, then closes", async () => {
    await openMenu();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    await expect.element(page.getByRole("option", { name: /Work/ })).toHaveAttribute("aria-selected", "true");
    await userEvent.keyboard("{ArrowDown}{ArrowUp}{Enter}");
    await expect.element(ran()).toHaveTextContent("park");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
  });

  it("filters as the query changes and starts again at the top", async () => {
    await openMenu();
    await userEvent.keyboard("{ArrowDown}wo");
    await expect.element(page.getByRole("option")).toHaveTextContent(/Work/);
    await expect.element(page.getByRole("option", { name: /Work/ })).toHaveAttribute("aria-selected", "true");
    await userEvent.keyboard("{Enter}");
    await expect.element(ran()).toHaveTextContent("work");
  });

  it("clears the selection's chip with Backspace in an empty search", async () => {
    await openMenu();
    await userEvent.keyboard("{Backspace}");
    await expect.element(page.getByText("T10.1 · Add the skill", { exact: true })).not.toBeInTheDocument();
    await expect.element(page.getByRole("group", { name: "Actions on T10.1" })).not.toBeInTheDocument();
  });

  it("closes with Escape without running anything, and gives focus back", async () => {
    await openMenu();
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog")).not.toBeInTheDocument();
    await expect.element(page.getByRole("button", { name: "Open menu" })).toHaveFocus();
    await expect.element(ran()).toHaveTextContent("");
  });

  it("opens again right after Escape", async () => {
    await openMenu();
    await userEvent.keyboard("{Escape}");
    await page.getByRole("button", { name: "Open menu" }).click();
    await expect.element(search()).toHaveFocus();
  });

  it("runs a command clicked with the pointer", async () => {
    await openMenu();
    await page.getByRole("option", { name: /Park/ }).click();
    await expect.element(ran()).toHaveTextContent("park");
  });
});
