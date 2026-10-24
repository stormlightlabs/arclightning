import { describe, expect, it } from "vitest";
import { selectedOf, taskHref, workHref } from "./links";

describe("Work list links", () => {
  it("returns to the Work list with its filters and the task selected", () => {
    const from = new URL("http://127.0.0.1:4000/?status=pending&milestone=ph-7&selected=t-1");
    expect(workHref("t 2", from)).toBe("/?status=pending&milestone=ph-7&selected=t+2");
  });

  it("drops the query of a page other than the Work list", () => {
    expect(workHref("t-2", new URL("http://127.0.0.1:4000/tasks/t-1?x=1"))).toBe("/?selected=t-2");
    expect(workHref("t-2")).toBe("/?selected=t-2");
  });

  it("reads the selected row back and escapes task links", () => {
    expect(selectedOf(new URLSearchParams(workHref("t 2").slice(2)))).toBe("t 2");
    expect(selectedOf(new URLSearchParams("selected="))).toBeNull();
    expect(taskHref("t/1")).toBe("/tasks/t%2F1");
  });
});
