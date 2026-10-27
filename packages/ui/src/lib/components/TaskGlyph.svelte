<script module lang="ts">
  import type { TaskGlyphKind } from "../types";
  /** Props for the small glyph that leads a task row. */
  export type TaskGlyphProps = {
    kind: TaskGlyphKind;
    label?: string;
  };
</script>

<script lang="ts">
  let { kind, label }: TaskGlyphProps = $props();
</script>

<svg
  class="arcl-task-glyph"
  data-kind={kind}
  width="14"
  height="14"
  viewBox="0 0 14 14"
  role={label ? "img" : undefined}
  aria-label={label}
  aria-hidden={label ? undefined : "true"}>
  {#if kind === "done"}
    <circle cx="7" cy="7" r="6" fill="currentColor" />
    <path d="m4.3 7.2 1.8 1.8 3.6-3.8" fill="none" stroke="var(--arcl-surface)" stroke-width="1.5" />
  {:else}
    <circle
      cx="7"
      cy="7"
      r="5.5"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-dasharray={kind === "waiting" ? "2 2" : undefined} />
    {#if kind === "ready"}<circle cx="7" cy="7" r="2.5" fill="currentColor" />{/if}
    {#if kind === "blocked"}<path d="M4.5 7h5" stroke="currentColor" stroke-width="1.5" />{/if}
    {#if kind === "in_progress"}<path d="M7 1.5a5.5 5.5 0 0 1 0 11z" fill="currentColor" />{/if}
    {#if kind === "cancelled"}<path d="m4.5 4.5 5 5" stroke="currentColor" stroke-width="1.5" />{/if}
  {/if}
</svg>

<style>
  .arcl-task-glyph {
    flex: none;
    color: var(--arcl-text-muted);
  }
  [data-kind="ready"],
  [data-kind="in_progress"] {
    color: var(--arcl-info);
  }
  [data-kind="blocked"] {
    color: var(--arcl-danger);
  }
  [data-kind="done"] {
    color: var(--arcl-success);
  }
</style>
