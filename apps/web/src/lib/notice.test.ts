import { describe, expect, it } from "vitest";
import { QUEUED } from "$lib/data/types";
import { failureNotice } from "./notice";

describe("failureNotice", () => {
  it("shows a failed change as an error with the CLI's code", () => {
    expect(failureNotice({ code: "not_found", message: "no such task" }, "Could not start this task")).toEqual({
      tone: "danger",
      title: "Could not start this task",
      message: "no such task (not_found)",
    });
  });

  it("shows a queued change as queued, not as saved or failed", () => {
    expect(failureNotice({ code: QUEUED, message: "Queued." }, "Could not start this task")).toEqual({
      tone: "info",
      title: "Queued for the next sync",
      message: "Queued.",
    });
  });
});
