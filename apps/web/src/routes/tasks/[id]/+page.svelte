<script lang="ts">
  import { EmptyState } from "@arclightning/ui";
  import { page } from "$app/state";
</script>

<svelte:head><title>Task · Arc Lightning</title></svelte:head>

<h1>{page.params.id}</h1>
<EmptyState title="The Task view is not built yet" message="Run arcl explain with this ID to see the task, its readiness and its blockers." />

<style>
  h1 { margin: 0 0 var(--arcl-space-4); color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui); }
</style>
