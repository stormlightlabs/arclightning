import { describe, expect, it } from "vitest";
import { isFiltered, matches, parseFilters, readinessOf, writeFilters } from "./filters";
import { agents, blocked, closed, ready } from "./fixtures";

describe("parseFilters", () => {
  it("reads repeated values and drops unknown ones", () => {
    const filters = parseFilters(new URLSearchParams("status=pending&status=bogus&readiness=ready&priority=high&milestone=ph-5&milestone="));
    expect(filters).toEqual({ status: ["pending"], readiness: ["ready"], priority: ["high"], milestone: ["ph-5"] });
  });
});

describe("writeFilters", () => {
  it("round-trips through the URL and keeps other parameters", () => {
    const filters = { status: ["parked" as const], readiness: [], priority: ["low" as const, "high" as const], milestone: ["none"] };
    const search = writeFilters(filters, new URLSearchParams("status=pending&view=all"));
    expect(search.toString()).toBe("view=all&status=parked&priority=low&priority=high&milestone=none");
    expect(parseFilters(search)).toEqual(filters);
  });

  it("drops a cleared filter from the URL", () => {
    const search = writeFilters(parseFilters(new URLSearchParams()), new URLSearchParams("readiness=ready"));
    expect(search.toString()).toBe("");
  });
});

describe("matches", () => {
  it("narrows by status, readiness, priority and milestone together", () => {
    const filters = parseFilters(new URLSearchParams("status=pending&readiness=ready&priority=high&milestone=ph-5"));
    expect(matches(ready("a", agents, { priority: "high" }), filters)).toBe(true);
    expect(matches(ready("a", agents), filters)).toBe(false);
    expect(matches(ready("a", null, { priority: "high" }), filters)).toBe(false);
    expect(matches(blocked("a", "b", agents, { priority: "high" }), filters)).toBe(false);
  });

  it("matches tasks outside any milestone with `none`", () => {
    const filters = parseFilters(new URLSearchParams("milestone=none"));
    expect(matches(ready("a", null), filters)).toBe(true);
    expect(matches(ready("a", agents), filters)).toBe(false);
  });

  it("puts no limit on an empty filter", () => {
    const filters = parseFilters(new URLSearchParams());
    expect(isFiltered(filters)).toBe(false);
    expect(matches(closed("a", "cancelled", null), filters)).toBe(true);
  });
});

describe("readinessOf", () => {
  it("tells ready, blocked and waiting apart", () => {
    expect(readinessOf(ready("a", null))).toBe("ready");
    expect(readinessOf(blocked("a", "b", null))).toBe("blocked");
    expect(readinessOf(closed("a", "parked", null))).toBe("waiting");
  });
});
