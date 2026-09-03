<script module lang="ts">
  /** Task priorities shown by shared planning components. */
  export type Priority = "low" | "medium" | "high" | "critical";
  /** Props for a priority label. */
  export interface PriorityBadgeProps { priority: Priority; }
</script>

<script lang="ts">
  import Badge, { type BadgeTone } from "./Badge.svelte";
  let { priority }: PriorityBadgeProps = $props();
  const tones: Record<Priority, BadgeTone> = { low: "neutral", medium: "info", high: "warning", critical: "danger" };
</script>

<Badge label={priority[0].toUpperCase() + priority.slice(1)} tone={tones[priority]} />
