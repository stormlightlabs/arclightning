import type { QueuedEdit, SyncStatus } from "$lib/data/types";

/** What each queued operation does, as a verb phrase. */
const VERBS: Record<string, string> = {
  create_task: "Create the task",
  update_task: "Edit the task",
  start_task: "Start the task",
  park_task: "Park the task",
  unpark_task: "Unpark the task",
  hand_off_task: "Hand off the task",
  complete_task: "Complete the task",
  create_capture: "Capture",
  promote_capture: "Promote the capture",
  discard_capture: "Discard the capture",
  create_spec: "Create the spec",
  create_plan: "Create the plan",
  apply_plan: "Apply the plan",
  update_source: "Edit the file of",
};

/**
 * Describe a queued edit in a line, such as `Discard the capture
 * arcl-c-1`. Its fields come from the shared queue, so this only quotes
 * them; the view sets the result as text.
 */
export function describeEdit(edit: QueuedEdit): string {
  const verb = VERBS[edit.op];
  if (verb === undefined) return edit.target === null ? `Run ${edit.op}` : `Run ${edit.op} on ${edit.target}`;
  const title = typeof edit.input.title === "string" && edit.input.title.trim() !== "" ? `“${edit.input.title}”` : null;
  const to = edit.op === "promote_capture" && typeof edit.input.to === "string" ? ` to a ${edit.input.to}` : "";
  const subject = title ?? edit.target;
  return `${subject === null ? verb : `${verb} ${subject}`}${to}`;
}

/**
 * Whether an edit to record `id` waits for the next sync, queued or being
 * applied. Applying it rewrites the record's file, so a raw edit made from
 * the file before then is stale.
 */
export function editWaitsFor(status: SyncStatus | null, id: string): boolean {
  return status?.edits.some((edit) => edit.target === id && edit.status !== "failed") ?? false;
}

/** A sync time for people, such as `Oct 25, 06:17 UTC`, or `null` before the first sync. */
export function syncedLabel(status: SyncStatus): string | null {
  if (status.syncedAt === null) return null;
  const date = new Date(status.syncedAt);
  if (Number.isNaN(date.getTime())) return null;
  const day = date.toLocaleDateString("en-US", { month: "short", day: "numeric", timeZone: "UTC" });
  const time = date.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit", timeZone: "UTC" });
  return `${day}, ${time} UTC`;
}
