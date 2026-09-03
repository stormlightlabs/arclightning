import { describe, expect, it } from "vitest";
import { formatStatus } from "../labels";

describe("formatStatus", () => {
  it("turns stored snake-case state into product copy", () => {
    expect(formatStatus("in_progress")).toBe("In progress");
  });
});
