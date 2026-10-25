<script module lang="ts">
  import type { SyncStatus } from "$lib/data";

  /** Props for the artifact's sync status. */
  export interface SyncStatusProps {
    status: SyncStatus;
    /** Whether a sync changed records since the view loaded. */
    stale?: boolean;
    /** Reload the view to show the records the last sync brought. */
    onreload?: () => void;
    /** Remove a failed edit from the queue. */
    ondismiss?: (id: string) => void;
  }
</script>

<script lang="ts">
  import { Button, Feedback } from "@arclightning/ui";
  import { describeEdit, syncedLabel } from "./edits";

  let { status, stale = false, onreload, ondismiss }: SyncStatusProps = $props();

  let queued = $derived(status.edits.filter((edit) => edit.status === "queued"));
  let failed = $derived(status.edits.filter((edit) => edit.status === "failed"));
  let synced = $derived(syncedLabel(status));
</script>

<section class="sync" aria-label="Sync">
  <p class="line">
    {synced === null ? "Not synced yet." : `Synced ${synced}`}{#if status.commit}{` at ${status.commit.slice(0, 7)}`}{/if}.
    {#if queued.length}{queued.length === 1 ? " 1 edit waits" : ` ${queued.length} edits wait`} for the next sync.{/if}
  </p>
  {#if queued.length}
    <ul aria-label="Queued edits">
      {#each queued as edit (edit.id)}<li>{describeEdit(edit)}</li>{/each}
    </ul>
  {/if}
  {#if stale}
    <Feedback tone="info" title="The project synced" message="This view shows the records from before the sync.">
      {#snippet actions()}<Button label="Reload view" onclick={onreload} />{/snippet}
    </Feedback>
  {/if}
  {#each failed as edit (edit.id)}
    <Feedback tone="danger" title={`Not applied: ${describeEdit(edit)}`} message={edit.error ? `${edit.error.message} (${edit.error.code})` : "arcl refused it."}>
      {#snippet actions()}<Button label="Dismiss" variant="ghost" onclick={() => ondismiss?.(edit.id)} />{/snippet}
    </Feedback>
  {/each}
</section>

<style>
  .sync { display: grid; gap: var(--arcl-space-2); margin-bottom: var(--arcl-space-4); font-size: var(--arcl-type-sm); }
  .line { margin: 0; color: var(--arcl-text-muted); }
  ul { margin: 0; padding-inline-start: var(--arcl-space-5); color: var(--arcl-text-muted); }
</style>
