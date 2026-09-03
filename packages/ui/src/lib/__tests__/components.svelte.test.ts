import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import InteractionHarness from "./InteractionHarness.svelte";

describe("shared interactions", () => {
  it("supports labelled fields and menu keyboard navigation", async () => {
    await render(InteractionHarness);

    const field = page.getByLabelText("Name");
    await field.fill("Reconcile workspace");
    await expect.element(page.getByText("Reconcile workspace")).toBeInTheDocument();

    await page.getByRole("button", { name: "More actions" }).click();
    const promote = page.getByRole("menuitem", { name: "Promote to task" });
    expect(document.activeElement).toBe(promote.element());
    await promote.click();
    await expect.element(page.getByText("Promote to task")).toBeInTheDocument();
  });

  it("moves through tabs with arrow keys", async () => {
    await render(InteractionHarness);
    const first = page.getByRole("tab", { name: "First" });
    first.element().focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect.element(page.getByRole("tab", { name: "Second" })).toHaveAttribute("aria-selected", "true");
    await expect.element(page.getByText("Second panel")).toBeInTheDocument();
  });

  it("opens a modal, closes with Escape, and restores focus", async () => {
    await render(InteractionHarness);
    const trigger = page.getByRole("button", { name: "Open dialog" });
    await trigger.click();
    await expect.element(page.getByRole("dialog", { name: "Confirm promotion" })).toBeVisible();
    await userEvent.keyboard("{Escape}");
    await expect.element(page.getByRole("dialog", { name: "Confirm promotion" })).not.toBeInTheDocument();
    expect(document.activeElement).toBe(trigger.element());
  });

  it("applies theme tokens and visible focus styling", async () => {
    await render(InteractionHarness);
    const trigger = page.getByRole("button", { name: "Open dialog" }).element();
    trigger.focus();
    expect(getComputedStyle(trigger).outlineStyle).toBe("solid");
    expect(getComputedStyle(document.querySelector("[data-arcl-theme]")!).colorScheme).toBe("light");
  });
});
