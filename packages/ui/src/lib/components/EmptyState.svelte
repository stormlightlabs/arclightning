<script module lang="ts">
  import type { Snippet } from "svelte";
  /** Props for a useful empty collection state. */
  export interface EmptyStateProps { title: string; message: string; actions?: Snippet; }
</script>

<script lang="ts">
  let { title, message, actions }: EmptyStateProps = $props();
</script>

<div class="arcl-empty-state">
  <span class="arcl-empty-state__mark" aria-hidden="true">✦</span>
  <h3>{title}</h3>
  <p>{message}</p>
  {#if actions}<div>{@render actions()}</div>{/if}
</div>

<style>
  .arcl-empty-state { display: grid; justify-items: start; gap: var(--arcl-space-2); padding: var(--arcl-space-6); border: 1px dashed var(--arcl-border); border-radius: var(--arcl-radius-lg); color: var(--arcl-text-muted); background: var(--arcl-surface-subtle); }
  .arcl-empty-state__mark { color: var(--arcl-accent-hover); font-size: 1.5rem; }
  h3 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-lg) / 1.25 var(--arcl-font-ui); }
  p { max-width: 48ch; margin: 0; }
</style>
