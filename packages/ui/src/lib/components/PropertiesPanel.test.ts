import { render } from "svelte/server";
import { describe, expect, it } from "vitest";
import PropertiesPanel, { type PropertyItem } from "./PropertiesPanel.svelte";

const properties: PropertyItem[] = [
  { label: "Status", value: "In progress", status: "in_progress" },
  { label: "Priority", value: "high", priority: "high" },
  { label: "Parent", value: "T10: Add the MCP server", href: "/tasks/t-10" },
  { label: "Spec", value: null },
];

describe("PropertiesPanel", () => {
  it("shows badges, links and None, and ties the button to the list", () => {
    const { body } = render(PropertiesPanel, { props: { properties, shortcut: "]" } });
    expect(body).toContain(">In progress<");
    expect(body).toContain(">High<");
    expect(body).toMatch(/<a href="\/tasks\/t-10"[^>]*>T10: Add the MCP server<\/a>/);
    expect(body).toContain(">None<");
    expect(body).toContain('aria-expanded="true"');
    expect(body).toContain('aria-keyshortcuts="]"');
    const controls = /aria-controls="([^"]+)"/.exec(body)?.[1];
    expect(body).toMatch(new RegExp(`<dl id="${controls}"(?![^>]*hidden)`));
  });

  it("hides the list when collapsed", () => {
    const { body } = render(PropertiesPanel, { props: { properties, open: false } });
    expect(body).toContain('aria-expanded="false"');
    expect(body).toMatch(/<dl[^>]*hidden/);
  });
});
