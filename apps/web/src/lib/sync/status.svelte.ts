import type { SyncStatus } from "$lib/data/types";

/** The status the layout follows, shared so a view can read the queue too. */
let current = $state<SyncStatus | null>(null);

/**
 * The artifact's last sync and queued edits, as the layout follows them, or
 * `null` in the `arcl serve` build and before the first update.
 */
export function syncStatus(): SyncStatus | null {
  return current;
}

/** Set the status views read through {@link syncStatus}. The layout calls it on each update. */
export function setSyncStatus(status: SyncStatus | null): void {
  current = status;
}
