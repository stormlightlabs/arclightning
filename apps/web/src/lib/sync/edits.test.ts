import { describe, expect, it } from "vitest";
import type { QueuedEdit } from "$lib/data";
import { describeEdit, syncedLabel } from "./edits";

const edit = (op: string, target: string | null, input: Record<string, unknown> = {}): QueuedEdit => ({
  id: "q1",
  op,
  target,
  input,
  created_at: "2026-10-25T06:30:00Z",
  status: "queued",
  error: null,
});

describe("describeEdit", () => {
  it("names the change and the record or title it applies to", () => {
    expect(describeEdit(edit("discard_capture", "arcl-c-1"))).toBe("Discard the capture arcl-c-1");
    expect(describeEdit(edit("promote_capture", "arcl-c-1", { to: "task", title: "Ship it" }))).toBe("Promote the capture “Ship it” to a task");
    expect(describeEdit(edit("create_capture", null, { title: "Idea" }))).toBe("Capture “Idea”");
    expect(describeEdit(edit("park_task", "arcl-t-1", { title: "  " }))).toBe("Park the task arcl-t-1");
    expect(describeEdit(edit("rename_everything", null))).toBe("Run rename_everything");
    expect(describeEdit(edit("rename_everything", "arcl-t-1"))).toBe("Run rename_everything on arcl-t-1");
  });
});

describe("syncedLabel", () => {
  it("shows the sync time in UTC, or nothing before the first sync", () => {
    const status = { syncedAt: "2026-10-25T06:17:12Z", commit: null, revision: null, edits: [], stopped: null };
    expect(syncedLabel(status)).toBe("Oct 25, 06:17 UTC");
    expect(syncedLabel({ ...status, syncedAt: null })).toBeNull();
    expect(syncedLabel({ ...status, syncedAt: "not a time" })).toBeNull();
  });
});
