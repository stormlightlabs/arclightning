<script module lang="ts">
  /** Props for a concise readiness explanation. */
  export interface ReadinessProps { ready: boolean; reasons?: string[]; }
</script>

<script lang="ts">
  let { ready, reasons = [] }: ReadinessProps = $props();
</script>

<section class="arcl-readiness" data-ready={ready} aria-label="Task readiness">
  <strong>{ready ? "Ready to start" : "Not ready"}</strong>
  {#if reasons.length}
    <ul>{#each reasons as reason}<li>{reason}</li>{/each}</ul>
  {:else if ready}<p>All blockers are complete.</p>{/if}
</section>

<style>
  .arcl-readiness { padding: var(--arcl-space-4); border-inline-start: 4px solid var(--arcl-warning); border-radius: var(--arcl-radius-sm); background: var(--arcl-warning-surface); }
  .arcl-readiness[data-ready="true"] { border-color: var(--arcl-success); background: var(--arcl-success-surface); }
  strong { color: var(--arcl-heading); font-family: var(--arcl-font-ui); }
  ul, p { margin: var(--arcl-space-2) 0 0; padding-inline-start: 1.25rem; color: var(--arcl-text-muted); }
  p { padding: 0; }
</style>
