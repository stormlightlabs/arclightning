<script module lang="ts">
  import type { PlanningRecordSummary } from "../types";
  /** Props for a compact, selectable planning record. */
  export interface RecordSummaryProps { record: PlanningRecordSummary; selected?: boolean; onselect?: (record: PlanningRecordSummary) => void; }
</script>

<script lang="ts">
  import PriorityBadge from "./PriorityBadge.svelte";
  import StatusBadge from "./StatusBadge.svelte";
  let { record, selected = false, onselect }: RecordSummaryProps = $props();
</script>

<article class="arcl-record" data-selected={selected || undefined}>
  <button type="button" onclick={() => onselect?.(record)} aria-pressed={selected}>
    <span class="arcl-record__kind">{record.kind}</span>
    <span class="arcl-record__title">{record.title}</span>
    {#if record.description}<span class="arcl-record__description">{record.description}</span>{/if}
    <span class="arcl-record__meta">
      {#if record.status}<StatusBadge status={record.status} />{/if}
      {#if record.priority}<PriorityBadge priority={record.priority} />{/if}
      {#if record.metadata}<span>{record.metadata}</span>{/if}
    </span>
  </button>
</article>

<style>
  .arcl-record { border-bottom: 1px solid var(--arcl-border); }
  .arcl-record button { display: grid; width: 100%; min-height: 4.5rem; gap: var(--arcl-space-1); padding: var(--arcl-space-3) var(--arcl-space-4); border: 0; border-inline-start: 4px solid transparent; color: var(--arcl-text); background: transparent; text-align: left; cursor: pointer; }
  .arcl-record button:hover { background: var(--arcl-surface-hover); }
  .arcl-record[data-selected] button { border-inline-start-color: var(--arcl-accent); background: var(--arcl-surface-subtle); }
  .arcl-record__kind { color: var(--arcl-text-muted); font: 650 var(--arcl-type-xs) / 1 var(--arcl-font-code); text-transform: uppercase; letter-spacing: 0.06em; }
  .arcl-record__title { color: var(--arcl-heading); font: 650 var(--arcl-type-base) / 1.3 var(--arcl-font-ui); }
  .arcl-record__description { max-width: 62ch; color: var(--arcl-text-muted); }
  .arcl-record__meta { display: flex; flex-wrap: wrap; align-items: center; gap: var(--arcl-space-2); margin-top: var(--arcl-space-1); color: var(--arcl-text-muted); font-size: var(--arcl-type-xs); }
</style>
