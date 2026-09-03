import type { RecordStatus } from "./components/StatusBadge.svelte";

const STATUS_LABELS: Record<RecordStatus, string> = {
  draft: "Draft",
  pending: "Pending",
  ready: "Ready",
  in_progress: "In progress",
  parked: "Parked",
  complete: "Complete",
  cancelled: "Cancelled",
  discarded: "Discarded",
};

/** Convert a stored lifecycle status into its user-facing label. */
export function formatStatus(status: RecordStatus): string {
  return STATUS_LABELS[status];
}
