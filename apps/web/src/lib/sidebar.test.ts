import { describe, expect, it } from "vitest";
import { SIDEBAR_STORAGE_KEY, rememberSidebar, sidebarCollapsed } from "./sidebar";

function memory(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear: () => values.clear(),
    getItem: (key) => values.get(key) ?? null,
    key: (index) => [...values.keys()][index] ?? null,
    removeItem: (key) => void values.delete(key),
    setItem: (key, value) => void values.set(key, value),
  };
}

describe("sidebar storage", () => {
  it("remembers a collapse and an expand", () => {
    const storage = memory();
    expect(sidebarCollapsed(() => storage)).toBe(false);
    rememberSidebar(() => storage, true);
    expect(storage.getItem(SIDEBAR_STORAGE_KEY)).toBe("collapsed");
    expect(sidebarCollapsed(() => storage)).toBe(true);
    rememberSidebar(() => storage, false);
    expect(sidebarCollapsed(() => storage)).toBe(false);
  });

  it("reads as expanded and forgets quietly when storage throws", () => {
    const denied = () => {
      throw new DOMException("The document is sandboxed", "SecurityError");
    };
    expect(sidebarCollapsed(denied)).toBe(false);
    expect(() => rememberSidebar(denied, true)).not.toThrow();
    const full = { ...memory(), setItem: () => denied() };
    expect(() => rememberSidebar(() => full, true)).not.toThrow();
  });
});
