<script module lang="ts">
  /** Semantic treatments supported by the compact badge. */
  export type BadgeTone = "neutral" | "info" | "success" | "warning" | "danger";
  /** Props for compact metadata and state labels. */
  export interface BadgeProps { label: string; tone?: BadgeTone; }
</script>

<script lang="ts">
  let { label, tone = "neutral" }: BadgeProps = $props();
</script>

<span class="arcl-badge" data-tone={tone}>{label}</span>

<style>
  .arcl-badge { display: inline-flex; min-height: 1.625rem; align-items: center; padding-inline: var(--arcl-space-2); border: 1px solid var(--arcl-border); border-radius: 999px; color: var(--arcl-text); background: var(--arcl-surface-subtle); font: 650 var(--arcl-type-xs) / 1 var(--arcl-font-ui); white-space: nowrap; }
  [data-tone="info"] { color: var(--arcl-info); background: var(--arcl-info-surface); border-color: var(--arcl-info); }
  [data-tone="success"] { color: var(--arcl-success); background: var(--arcl-success-surface); border-color: var(--arcl-success); }
  [data-tone="warning"] { color: var(--arcl-warning); background: var(--arcl-warning-surface); border-color: var(--arcl-warning); }
  [data-tone="danger"] { color: var(--arcl-danger); background: var(--arcl-danger-surface); border-color: var(--arcl-danger); }
</style>
