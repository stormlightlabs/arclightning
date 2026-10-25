import { page } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import Markdown from "../components/Markdown.svelte";

describe("Markdown", () => {
  it("renders task list items as read-only checkboxes without a bullet", async () => {
    await render(Markdown, { source: "- [x] Ship it\n- [ ] Write the docs\n- Plain item\n\n1. [X] Loose\n\n2. [ ] Second\n" });
    const boxes = page.getByRole("checkbox").elements() as HTMLInputElement[];
    expect(boxes.map((box) => box.checked)).toEqual([true, false, true, false]);
    expect(boxes.every((box) => box.disabled)).toBe(true);
    await expect.element(page.getByRole("listitem").filter({ hasText: "Ship it" })).toHaveTextContent(/^\s*Ship it$/);
    for (const box of boxes) expect(getComputedStyle(box.closest("li")!).listStyleType).toBe("none");
    expect(getComputedStyle(page.getByRole("listitem").filter({ hasText: "Plain item" }).element()).listStyleType).toBe("disc");
  });

  it("still escapes raw HTML and drops unsafe link targets", async () => {
    await render(Markdown, { source: '- [ ] <img src=x onerror="alert(1)"> [link](javascript:alert(1))' });
    await expect.element(page.getByRole("listitem")).toHaveTextContent('<img src=x onerror="alert(1)"> link');
    expect(document.querySelector(".arcl-markdown img")).toBeNull();
    expect(page.getByRole("link", { name: "link" }).element().getAttribute("href")).toBe("");
  });
});
