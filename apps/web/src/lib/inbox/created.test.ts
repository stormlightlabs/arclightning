import { describe, expect, it } from "vitest";
import { createdAt, shortAge } from "./created";

describe("createdAt", () => {
  it("reads the creation time from the ULID that ends a record ID", () => {
    // The workspace file of this capture says `created-at = "2026-10-24T20:32:35.079Z"`.
    expect(createdAt("arcl-c-01M5QSTZR7D5Y37H635KD8NA44")?.toISOString()).toBe("2026-10-24T20:32:35.079Z");
    expect(createdAt("arcl-c-01m5qstzr7d5y37h635kd8na44")?.toISOString()).toBe("2026-10-24T20:32:35.079Z");
  });

  it("is null for an ID without a ULID", () => {
    expect(createdAt("arcl-c-1")).toBeNull();
    expect(createdAt("c-1")).toBeNull();
    // `U` is not in Crockford's alphabet, and a ULID never starts above 7.
    expect(createdAt("arcl-c-01M5QSTZR7D5Y37H635KD8NAUU")).toBeNull();
    expect(createdAt("arcl-c-81M5QSTZR7D5Y37H635KD8NA44")).toBeNull();
  });
});

describe("shortAge", () => {
  const now = new Date("2026-10-25T12:00:00Z");
  const before = (ms: number) => new Date(now.getTime() - ms);

  it("counts minutes, hours and days for a week", () => {
    expect(shortAge(before(10_000), now)).toBe("just now");
    expect(shortAge(before(5 * 60_000), now)).toBe("5m ago");
    expect(shortAge(before(3 * 3_600_000 + 59 * 60_000), now)).toBe("3h ago");
    expect(shortAge(before(6 * 86_400_000), now)).toBe("6d ago");
  });

  it("shows the date after a week, with the year once it differs, and for the future", () => {
    expect(shortAge(new Date("2026-10-10T12:00:00Z"), now)).toBe("Oct 10");
    expect(shortAge(new Date("2025-12-31T12:00:00Z"), now)).toBe("Dec 31, 2025");
    expect(shortAge(new Date("2026-10-26T12:00:00Z"), now)).toBe("Oct 26");
  });
});
