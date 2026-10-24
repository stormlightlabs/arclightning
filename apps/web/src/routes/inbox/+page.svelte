<script lang="ts">
  import { EmptyState } from "@arclightning/ui";
</script>

<svelte:head><title>Inbox · Arc Lightning</title></svelte:head>

<h1>Inbox</h1>
<EmptyState title="Nothing here yet" message="Captures you have not sorted yet appear here." />

<style>
  h1 { margin: 0 0 var(--arcl-space-4); color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui); }
</style>
