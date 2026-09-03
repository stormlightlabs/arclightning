<script module lang="ts">
  /** Lifecycle states shared by planning records. */
  export type RecordStatus = "draft" | "pending" | "ready" | "in_progress" | "parked" | "complete" | "cancelled" | "discarded";
  /** Props for a human-readable record status. */
  export interface StatusBadgeProps { status: RecordStatus; }
</script>

<script lang="ts">
  import { formatStatus } from "../labels";
  import Badge, { type BadgeTone } from "./Badge.svelte";
  let { status }: StatusBadgeProps = $props();
  const tones: Record<RecordStatus, BadgeTone> = { draft: "neutral", pending: "neutral", ready: "info", in_progress: "warning", parked: "warning", complete: "success", cancelled: "danger", discarded: "neutral" };
</script>

<Badge label={formatStatus(status)} tone={tones[status]} />
