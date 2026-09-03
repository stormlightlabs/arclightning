<script module lang="ts">
  import type { Snippet } from "svelte";
  /** Feedback states for inline and page-level notices. */
  export type FeedbackTone = "info" | "success" | "warning" | "danger";
  /** Props for actionable user feedback. */
  export interface FeedbackProps { title: string; message?: string; tone?: FeedbackTone; actions?: Snippet; }
</script>

<script lang="ts">
  let { title, message, tone = "info", actions }: FeedbackProps = $props();
</script>

<div class="arcl-feedback" data-tone={tone} role={tone === "danger" ? "alert" : "status"}>
  <div><strong>{title}</strong>{#if message}<p>{message}</p>{/if}</div>
  {#if actions}<div class="arcl-feedback__actions">{@render actions()}</div>{/if}
</div>

<style>
  .arcl-feedback { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--arcl-space-4); padding: var(--arcl-space-4); border-inline-start: 4px solid var(--arcl-info); border-radius: var(--arcl-radius-sm); color: var(--arcl-text); background: var(--arcl-info-surface); }
  .arcl-feedback[data-tone="success"] { border-color: var(--arcl-success); background: var(--arcl-success-surface); }
  .arcl-feedback[data-tone="warning"] { border-color: var(--arcl-warning); background: var(--arcl-warning-surface); }
  .arcl-feedback[data-tone="danger"] { border-color: var(--arcl-danger); background: var(--arcl-danger-surface); }
  strong { font-family: var(--arcl-font-ui); }
  p { margin: var(--arcl-space-1) 0 0; color: var(--arcl-text-muted); }
  .arcl-feedback__actions { flex: none; }
  @media (max-width: 32rem) { .arcl-feedback { flex-direction: column; } }
</style>
