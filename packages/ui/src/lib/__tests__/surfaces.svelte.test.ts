import { page, userEvent } from "vitest/browser";
import { describe, expect, it } from "vitest";
import { render } from "vitest-browser-svelte";
import "../styles/index.css";
import Badge from "../components/Badge.svelte";
import Button from "../components/Button.svelte";
import PropertiesPanel from "../components/PropertiesPanel.svelte";
import Readiness from "../components/Readiness.svelte";
import SelectMenu from "../components/SelectMenu.svelte";

describe("surfaces", () => {
  it("draws the readiness callout with a 1 px border on every side", async () => {
    await render(Readiness, { ready: false, reasons: ["blocked by T1"] });
    const style = getComputedStyle(page.getByRole("region", { name: "Task readiness" }).element());
    for (const side of ["Top", "Right", "Bottom", "Left"] as const) {
      expect(style[`border${side}Width`]).toBe("1px");
      expect(style[`border${side}Style`]).toBe("solid");
    }
  });

  it("keeps the properties header's hover inside the panel's rounded corners", async () => {
    await render(PropertiesPanel, { properties: [{ label: "Status", value: "Pending" }], shortcut: "]" });
    const panel = page.getByRole("complementary", { name: "Properties" }).element();
    const header = page.getByRole("button", { name: /Properties/ });
    await userEvent.hover(header);
    const outer = parseFloat(getComputedStyle(panel).borderTopLeftRadius);
    const border = parseFloat(getComputedStyle(panel).borderTopWidth);
    const open = getComputedStyle(header.element());
    expect(open.backgroundColor).not.toBe("rgba(0, 0, 0, 0)");
    expect(parseFloat(open.borderTopLeftRadius)).toBe(outer - border);
    expect(parseFloat(open.borderTopRightRadius)).toBe(outer - border);
    expect(open.borderBottomLeftRadius).toBe("0px");
    // Closed, the header is the whole panel, so every corner rounds.
    await header.click();
    expect(parseFloat(getComputedStyle(header.element()).borderBottomLeftRadius)).toBe(outer - border);
  });

  it("spaces a button's icon from its label with the button's gap, and swaps it for the spinner while busy", async () => {
    const { rerender } = await render(Button, { label: "Edit", icon: "edit" });
    const button = page.getByRole("button", { name: "Edit" }).element();
    const icon = button.querySelector(".arcl-icon")!;
    expect(icon.getAttribute("aria-hidden")).toBe("true");
    expect(icon.parentElement).toBe(button);
    const label = icon.nextElementSibling!;
    expect(label.textContent).toBe("Edit");
    const gap = parseFloat(getComputedStyle(button).columnGap);
    expect(gap).toBeGreaterThan(0);
    expect(label.getBoundingClientRect().left - icon.getBoundingClientRect().right).toBeCloseTo(gap, 0);

    await rerender({ label: "Edit", icon: "edit", busy: true });
    expect(button.querySelector(".arcl-button__spinner")).not.toBeNull();
    expect(button.querySelector(".arcl-icon")).toBeNull();
  });

  it("gives buttons and select triggers a 6 px radius and keeps badges round", async () => {
    await render(Button, { label: "Promote" });
    expect(getComputedStyle(page.getByRole("button", { name: "Promote" }).element()).borderRadius).toBe("6px");
    await render(SelectMenu, { label: "Status", options: [{ value: "", label: "Any" }], value: "" });
    expect(getComputedStyle(page.getByRole("combobox", { name: /Status/ }).element()).borderRadius).toBe("6px");
    await render(Badge, { label: "High" });
    expect(
      parseFloat(getComputedStyle(page.getByText("High").element().closest(".arcl-badge")!).borderRadius),
    ).toBeGreaterThan(100);
  });
});
