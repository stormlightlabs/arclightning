import type { RecordStatus } from "./components/StatusBadge.svelte";
import type { TaskGlyphKind } from "./types";

const STATUS_LABELS: Record<RecordStatus, string> = {
  draft: "Draft",
  pending: "Pending",
  ready: "Ready",
  in_progress: "In progress",
  parked: "Parked",
  completed: "Completed",
  cancelled: "Cancelled",
  discarded: "Discarded",
};

/** Convert a stored lifecycle status into its user-facing label. */
export function formatStatus(status: RecordStatus): string {
  return STATUS_LABELS[status];
}

const GLYPH_LABELS: Record<TaskGlyphKind, string> = {
  ready: "Ready",
  blocked: "Blocked",
  waiting: "Not ready",
  in_progress: "In progress",
  done: "Done",
  cancelled: "Cancelled",
};

/** The words a task glyph stands for. */
export function formatGlyph(kind: TaskGlyphKind): string {
  return GLYPH_LABELS[kind];
}
