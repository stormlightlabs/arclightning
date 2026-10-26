<script module lang="ts">
  import type { TaskRowItem } from "../types";
  /** Props for one dense, linkable task row. */
  export interface TaskRowProps {
    row: TaskRowItem;
    href: string;
    selected?: boolean;
    /** Whether Tab reaches this row; a list makes one row tabbable. */
    tabbable?: boolean;
    onselect?: (id: string) => void;
  }
</script>

<script lang="ts">
  import { formatGlyph } from "../labels";
  import PriorityBadge from "./PriorityBadge.svelte";
  import Readiness from "./Readiness.svelte";
  import StatusBadge from "./StatusBadge.svelte";
  import TaskGlyph from "./TaskGlyph.svelte";

  let { row, href, selected = false, tabbable = true, onselect }: TaskRowProps = $props();
  const reasonId = $props.id();
  // Readiness says whether work can start, so only rows not yet started or closed explain it.
  let explained = $derived(row.status === "pending" || row.status === "parked");
</script>

<li class="arcl-task-row" data-task-id={row.id} data-selected={selected || undefined}>
  <span class="arcl-task-row__glyph">
    <TaskGlyph kind={row.glyph} label={formatGlyph(row.glyph)} />
    {#if explained}
      <span class="arcl-task-row__reason" id={reasonId} role="tooltip">
        <Readiness ready={row.ready} reasons={row.reasons} />
      </span>
    {/if}
  </span>
  <a
    class="arcl-task-row__title"
    {href}
    aria-describedby={explained ? reasonId : undefined}
    aria-current={selected ? "true" : undefined}
    tabindex={tabbable ? 0 : -1}
    onfocus={() => onselect?.(row.id)}>{row.title}</a
  >
  <span class="arcl-task-row__meta">
    {#if row.priority !== "normal"}<PriorityBadge priority={row.priority} />{/if}
    <StatusBadge status={row.status} />
  </span>
</li>

<style>
  .arcl-task-row {
    position: relative;
    display: grid;
    grid-template-columns: 1rem minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--arcl-space-3);
    min-height: 2.25rem;
    padding: 0 var(--arcl-space-4);
    border-bottom: var(--arcl-border-width) solid var(--arcl-surface-subtle);
    font-size: var(--arcl-type-sm);
  }
  .arcl-task-row:hover {
    background: var(--arcl-surface-hover);
  }
  .arcl-task-row[data-selected] {
    background: var(--arcl-surface-subtle);
    box-shadow: inset 2px 0 0 var(--arcl-link);
  }
  .arcl-task-row__glyph {
    display: inline-flex;
  }
  .arcl-task-row__title {
    overflow: hidden;
    color: var(--arcl-text);
    text-decoration: none;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arcl-task-row__title:focus-visible {
    outline-offset: 0;
  }
  .arcl-task-row__meta {
    display: flex;
    gap: var(--arcl-space-2);
  }
  .arcl-task-row__reason {
    position: absolute;
    z-index: 1;
    top: calc(100% - var(--arcl-space-1));
    left: var(--arcl-space-2);
    display: none;
    width: min(24rem, calc(100vw - 2rem));
    border-radius: var(--arcl-radius-sm);
    background: var(--arcl-surface);
    box-shadow: var(--arcl-shadow-overlay);
  }
  .arcl-task-row__glyph:hover .arcl-task-row__reason,
  .arcl-task-row:focus-within .arcl-task-row__reason {
    display: block;
  }
</style>
