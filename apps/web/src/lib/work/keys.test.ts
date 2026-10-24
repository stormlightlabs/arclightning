import { describe, expect, it } from "vitest";
import { moveSelection, workKeyAction, type WorkKey } from "./keys";

/** A stand-in element that matches the selectors in `matching`. */
function element(...matching: string[]): EventTarget {
  return { closest: (selector: string) => (matching.some((tag) => selector.split(", ").some((part) => part.startsWith(tag))) ? {} : null) } as unknown as EventTarget;
}

function press(key: string, fields: Partial<WorkKey> = {}): WorkKey {
  return { key, altKey: false, ctrlKey: false, metaKey: false, target: null, ...fields };
}

describe("workKeyAction", () => {
  it("maps j, k and the arrows to moves, and Enter to open", () => {
    expect(workKeyAction(press("j"))).toBe("next");
    expect(workKeyAction(press("ArrowDown"))).toBe("next");
    expect(workKeyAction(press("k"))).toBe("previous");
    expect(workKeyAction(press("ArrowUp"))).toBe("previous");
    expect(workKeyAction(press("Enter"))).toBe("open");
    expect(workKeyAction(press("x"))).toBeNull();
  });

  it("leaves keys held with a modifier alone", () => {
    expect(workKeyAction(press("j", { metaKey: true }))).toBeNull();
    expect(workKeyAction(press("k", { ctrlKey: true }))).toBeNull();
    expect(workKeyAction(press("j", { altKey: true }))).toBeNull();
  });

  it("leaves keys typed into form fields alone", () => {
    expect(workKeyAction(press("j", { target: element("select") }))).toBeNull();
    expect(workKeyAction(press("k", { target: element("input") }))).toBeNull();
    expect(workKeyAction(press("ArrowDown", { target: element("[role=combobox]") }))).toBeNull();
    expect(workKeyAction(press("Enter", { target: element("[role=listbox]") }))).toBeNull();
  });

  it("lets a focused link open itself on Enter but still moves on j", () => {
    expect(workKeyAction(press("Enter", { target: element("a") }))).toBeNull();
    expect(workKeyAction(press("j", { target: element("a") }))).toBe("next");
  });
});

describe("moveSelection", () => {
  const order = ["a", "b", "c"];

  it("starts at the first row going down and the last going up", () => {
    expect(moveSelection(order, null, 1)).toBe("a");
    expect(moveSelection(order, null, -1)).toBe("c");
    expect(moveSelection(order, "gone", 1)).toBe("a");
  });

  it("moves one row and stops at the ends", () => {
    expect(moveSelection(order, "a", 1)).toBe("b");
    expect(moveSelection(order, "b", -1)).toBe("a");
    expect(moveSelection(order, "c", 1)).toBe("c");
    expect(moveSelection(order, "a", -1)).toBe("a");
  });

  it("selects nothing in an empty list", () => {
    expect(moveSelection([], "a", 1)).toBeNull();
  });
});
