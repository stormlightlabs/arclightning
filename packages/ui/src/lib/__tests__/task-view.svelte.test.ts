import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import TaskViewHarness from "./TaskViewHarness.svelte";

describe("PropertiesPanel", () => {
  it("collapses and expands from the keyboard", async () => {
    await render(TaskViewHarness);
    const toggle = page.getByRole("button", { name: /Properties/ });
    await expect.element(toggle).toHaveAttribute("aria-expanded", "true");
    await expect.element(page.getByText("Status")).toBeVisible();
    toggle.element().focus();
    await userEvent.keyboard("{Enter}");
    await expect.element(toggle).toHaveAttribute("aria-expanded", "false");
    await expect.element(page.getByText("Status")).not.toBeVisible();
    await userEvent.keyboard(" ");
    await expect.element(toggle).toHaveAttribute("aria-expanded", "true");
  });
});

describe("MarkdownEditor", () => {
  it("shows the Markdown in CodeMirror and refuses edits when read-only", async () => {
    await render(TaskViewHarness);
    const editor = page.getByRole("textbox", { name: "Description Markdown" });
    await expect.element(editor).toHaveAttribute("aria-readonly", "true");
    await expect.element(editor).toHaveTextContent("Some **bold** text.");
    editor.element().focus();
    await userEvent.keyboard("xyz");
    await expect.element(editor).not.toHaveTextContent("xyz");
    await expect.element(page.getByText("0 edits")).toBeInTheDocument();
  });

  it("reports edits when writable", async () => {
    await render(TaskViewHarness, { readonly: false });
    const editor = page.getByRole("textbox", { name: "Description Markdown" });
    editor.element().focus();
    await userEvent.keyboard("x");
    await expect.element(page.getByText("1 edits")).toBeInTheDocument();
  });

  it("gives up focus on Escape", async () => {
    await render(TaskViewHarness);
    const editor = page.getByRole("textbox", { name: "Description Markdown" });
    editor.element().focus();
    await expect.element(editor).toHaveFocus();
    await userEvent.keyboard("{Escape}");
    await expect.element(editor).not.toHaveFocus();
  });
});
