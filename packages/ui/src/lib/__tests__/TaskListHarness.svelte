<script lang="ts">
  import TaskList from "../components/TaskList.svelte";
  import type { TaskGroup } from "../types";

  let { groups }: { groups: TaskGroup[] } = $props();
  let selectedId = $state<string | null>(null);
</script>

<div data-arcl-theme="light">
  <button type="button" onclick={() => (selectedId = "t-2")}>Select second</button>
  <TaskList {groups} href={(id) => `#${id}`} {selectedId} onselect={(id) => (selectedId = id)} />
  <output>{selectedId}</output>
</div>
