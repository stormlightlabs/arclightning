import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/** WCAG AA's minimum ratio for normal text. */
const TEXT = 4.5;
/** WCAG AA's minimum ratio for UI components and focus indicators. */
const UI = 3;

const SURFACES = ["canvas", "surface", "surface-subtle", "surface-hover"];

/** Each `[foreground, background, minimum]` pair the components draw. */
const PAIRS: [string, string, number][] = [
  ...["text", "text-muted", "heading", "link"].flatMap((fg) => SURFACES.map((bg): [string, string, number] => [fg, bg, TEXT])),
  ["on-accent", "accent", TEXT],
  ["on-accent", "accent-hover", TEXT],
  ...["info", "success", "warning", "danger"].flatMap((tone) =>
    [`${tone}-surface`, "surface", "canvas"].map((bg): [string, string, number] => [tone, bg, TEXT]),
  ),
  ...["border", "border-strong", "focus", "accent"].flatMap((fg) =>
    ["canvas", "surface", "surface-subtle"].map((bg): [string, string, number] => [fg, bg, UI]),
  ),
];

const css = readFileSync(new URL("./tokens.css", import.meta.url), "utf8");

/** The `--arcl-*` declarations in the first block that follows `selector`. */
function block(selector: string): Record<string, string> {
  const start = css.indexOf(selector);
  if (start < 0) throw new Error(`tokens.css has no ${selector}`);
  const open = css.indexOf("{", start + selector.length);
  const body = css.slice(open + 1, css.indexOf("}", open));
  return Object.fromEntries([...body.matchAll(/--arcl-([a-z-]+):\s*([^;]+);/g)].map(([, name, value]) => [name, value.trim()]));
}

/** The relative luminance of a `#rrggbb` color. */
function luminance(hex: string): number {
  const match = /^#([0-9a-f]{6})$/i.exec(hex);
  if (!match) throw new Error(`${hex} is not a #rrggbb color`);
  const [r, g, b] = [0, 2, 4].map((at) => {
    const channel = parseInt(match[1].slice(at, at + 2), 16) / 255;
    return channel <= 0.04045 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The WCAG contrast ratio of two `#rrggbb` colors. */
function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
}

const light = block('[data-arcl-theme="light"]');
const dark = { ...light, ...block('[data-arcl-theme="dark"]') };

describe("theme tokens", () => {
  it("computes contrast as WCAG defines it", () => {
    expect(contrast("#000000", "#ffffff")).toBeCloseTo(21);
    expect(contrast("#777777", "#ffffff")).toBeCloseTo(4.48, 2);
  });

  for (const [name, theme] of [
    ["light", light],
    ["dark", dark],
  ] as const) {
    it.each(PAIRS)(`${name}: %s on %s meets %d:1`, (fg, bg, minimum) => {
      expect(contrast(theme[fg], theme[bg])).toBeGreaterThanOrEqual(minimum);
    });
  }

  it("gives the system dark setting the same tokens as the dark theme", () => {
    expect(block(":root:not([data-arcl-theme])")).toEqual(block('[data-arcl-theme="dark"]'));
  });
});
