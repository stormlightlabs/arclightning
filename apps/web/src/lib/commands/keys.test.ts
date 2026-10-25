import { describe, expect, it } from "vitest";
import { formatKeys, isMac, opensMenu, shortcutStep, takesShortcut, type ShortcutKey } from "./keys";

/** A stand-in element that matches the selectors in `matching`. */
function element(...matching: string[]): EventTarget {
  return { closest: (selector: string) => (matching.some((tag) => selector.split(", ").some((part) => part.startsWith(tag))) ? {} : null) } as unknown as EventTarget;
}

function press(key: string, fields: Partial<ShortcutKey> = {}): ShortcutKey {
  return { key, altKey: false, ctrlKey: false, metaKey: false, repeat: false, defaultPrevented: false, target: null, ...fields };
}

const commands = [
  { keys: ["q"], label: "Quick capture" },
  { keys: ["s"], label: "Start" },
  { keys: ["g", "w"], label: "Work" },
  { keys: ["g", "i"], label: "Inbox" },
];

describe("opensMenu", () => {
  it("takes ⌘K on macOS and Ctrl+K elsewhere", () => {
    expect(opensMenu(press("k", { metaKey: true }), true)).toBe(true);
    expect(opensMenu(press("K", { metaKey: true }), true)).toBe(true);
    expect(opensMenu(press("k", { ctrlKey: true }), true)).toBe(false);
    expect(opensMenu(press("k", { ctrlKey: true }), false)).toBe(true);
    expect(opensMenu(press("k", { metaKey: true }), false)).toBe(false);
    expect(opensMenu(press("k"), false)).toBe(false);
    expect(opensMenu(press("k", { ctrlKey: true, altKey: true }), false)).toBe(false);
  });

  it("leaves the key to form fields, the editor and dialogs", () => {
    for (const target of ["input", "textarea", "[contenteditable]", "dialog", "[role=combobox]"]) {
      expect(opensMenu(press("k", { ctrlKey: true, target: element(target) }), false), target).toBe(false);
    }
    expect(opensMenu(press("k", { ctrlKey: true, defaultPrevented: true }), false)).toBe(false);
  });

  it("opens once for a held key", () => {
    expect(opensMenu(press("k", { ctrlKey: true, repeat: true }), false)).toBe(false);
  });
});

describe("takesShortcut", () => {
  it("answers plain keys outside fields", () => {
    expect(takesShortcut(press("q"))).toBe(true);
    expect(takesShortcut(press("q", { target: element("main") }))).toBe(true);
  });

  it("leaves keys typed into fields, the editor or a dialog alone", () => {
    for (const target of ["input", "textarea", "select", "[contenteditable]", "dialog", "[role=listbox]"]) {
      expect(takesShortcut(press("q", { target: element(target) })), target).toBe(false);
    }
  });

  it("leaves keys held with a modifier, or already handled, alone", () => {
    expect(takesShortcut(press("q", { metaKey: true }))).toBe(false);
    expect(takesShortcut(press("q", { ctrlKey: true }))).toBe(false);
    expect(takesShortcut(press("q", { altKey: true }))).toBe(false);
    expect(takesShortcut(press("q", { defaultPrevented: true }))).toBe(false);
  });

  it("ignores the repeats of a held key", () => {
    expect(takesShortcut(press("c", { repeat: true }))).toBe(false);
  });
});

describe("shortcutStep", () => {
  it("matches a single key", () => {
    expect(shortcutStep([], "q", commands)).toEqual({ match: commands[0], pending: [] });
    expect(shortcutStep([], "x", commands)).toEqual({ match: null, pending: [] });
  });

  it("waits for the second key of a sequence", () => {
    const first = shortcutStep([], "g", commands);
    expect(first).toEqual({ match: null, pending: ["g"] });
    expect(shortcutStep(first.pending, "w", commands).match?.label).toBe("Work");
    expect(shortcutStep(first.pending, "i", commands).match?.label).toBe("Inbox");
  });

  it("starts over when the next key does not continue the sequence", () => {
    expect(shortcutStep(["g"], "s", commands)).toEqual({ match: commands[1], pending: [] });
    expect(shortcutStep(["g"], "x", commands)).toEqual({ match: null, pending: [] });
  });
});

describe("formatKeys", () => {
  it("names Mod for the platform and capitalizes letters", () => {
    expect(formatKeys(["Mod", "k"], true)).toEqual(["⌘", "K"]);
    expect(formatKeys(["Mod", "k"], false)).toEqual(["Ctrl", "K"]);
    expect(formatKeys(["g", "w", "Enter", "Escape", "]", "Backspace", "1"], false)).toEqual(["G", "W", "↵", "Esc", "]", "⌫", "1"]);
  });

  it("tells macOS by its platform string", () => {
    expect(isMac("MacIntel")).toBe(true);
    expect(isMac("iPhone")).toBe(true);
    expect(isMac("Win32")).toBe(false);
    expect(isMac("Linux x86_64")).toBe(false);
  });
});
