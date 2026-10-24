import { describe, expect, it } from "vitest";
import { listboxKey, typeahead, type ListboxKey, type ListboxOption, type ListboxState } from "./listbox";

const options: ListboxOption[] = [
  { label: "Any" },
  { label: "Pending" },
  { label: "Parked", disabled: true },
  { label: "Planned" },
  { label: "Completed" },
];

const press = (key: string, fields: Partial<ListboxKey> = {}): ListboxKey => ({ key, altKey: false, ctrlKey: false, metaKey: false, ...fields });
const closed: ListboxState = { open: false, active: 1, searching: false };
const open = (active: number, searching = false): ListboxState => ({ open: true, active, searching });

describe("listboxKey while closed", () => {
  it("opens on the active option with Down, Up, Enter and Space", () => {
    for (const key of ["ArrowDown", "ArrowUp", "Enter", " "]) {
      expect(listboxKey(press(key), closed, options)).toEqual({ open: true, active: 1, commit: false, prevent: true });
    }
  });

  it("opens on the first or last option with Home and End", () => {
    expect(listboxKey(press("Home"), closed, options)).toMatchObject({ open: true, active: 0 });
    expect(listboxKey(press("End"), closed, options)).toMatchObject({ open: true, active: 4 });
  });

  it("opens and starts type-ahead on a character", () => {
    expect(listboxKey(press("c"), closed, options)).toMatchObject({ open: true, type: "c" });
  });

  it("leaves Tab, Escape and modified keys alone", () => {
    expect(listboxKey(press("Tab"), closed, options)).toBeNull();
    expect(listboxKey(press("Escape"), closed, options)).toBeNull();
    expect(listboxKey(press("ArrowDown", { metaKey: true }), closed, options)).toBeNull();
    expect(listboxKey(press("c", { ctrlKey: true }), closed, options)).toBeNull();
  });
});

describe("listboxKey while open", () => {
  it("moves one option, skips disabled ones and stops at the ends", () => {
    expect(listboxKey(press("ArrowDown"), open(1), options)).toMatchObject({ open: true, active: 3 });
    expect(listboxKey(press("ArrowUp"), open(3), options)).toMatchObject({ active: 1 });
    expect(listboxKey(press("ArrowDown"), open(4), options)).toMatchObject({ active: 4 });
    expect(listboxKey(press("ArrowUp"), open(0), options)).toMatchObject({ active: 0 });
  });

  it("jumps with Home, End, Page Up and Page Down", () => {
    expect(listboxKey(press("Home"), open(3), options)).toMatchObject({ active: 0 });
    expect(listboxKey(press("End"), open(0), options)).toMatchObject({ active: 4 });
    expect(listboxKey(press("PageDown"), open(0), options)).toMatchObject({ active: 4 });
    expect(listboxKey(press("PageUp"), open(4), options)).toMatchObject({ active: 0 });
  });

  it("chooses the active option with Enter, Space and Alt+Up", () => {
    for (const event of [press("Enter"), press(" "), press("ArrowUp", { altKey: true })]) {
      expect(listboxKey(event, open(3), options)).toEqual({ open: false, active: 3, commit: true, prevent: true });
    }
  });

  it("adds Space to a type-ahead query in progress instead of choosing", () => {
    expect(listboxKey(press(" "), open(3, true), options)).toMatchObject({ open: true, commit: false, type: " " });
  });

  it("closes without choosing on Escape", () => {
    expect(listboxKey(press("Escape"), open(3), options)).toEqual({ open: false, active: 3, commit: false, prevent: true });
  });

  it("chooses on Tab and lets focus move on", () => {
    expect(listboxKey(press("Tab"), open(3), options)).toEqual({ open: false, active: 3, commit: true, prevent: false });
  });
});

describe("typeahead", () => {
  it("finds the next option starting with the query, ignoring case and disabled options", () => {
    expect(typeahead(options, 0, "p")).toBe(1);
    expect(typeahead(options, 1, "P")).toBe(3);
    expect(typeahead(options, 0, "pa")).toBeNull();
  });

  it("cycles through matches when one letter repeats, wrapping at the end", () => {
    expect(typeahead(options, 1, "pp")).toBe(3);
    expect(typeahead(options, 3, "ppp")).toBe(1);
  });

  it("keeps the active option while a longer query still matches it", () => {
    expect(typeahead(options, 3, "pl")).toBe(3);
    expect(typeahead(options, 1, "pl")).toBe(3);
  });

  it("matches nothing in an empty list", () => {
    expect(typeahead([], 0, "a")).toBeNull();
  });
});
