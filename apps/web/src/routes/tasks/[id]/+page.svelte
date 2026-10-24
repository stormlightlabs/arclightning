<script lang="ts">
  import { Feedback } from "@arclightning/ui";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import { dataSource, type TaskDetail } from "$lib/data";
  import { taskKeyAction } from "$lib/task";
  import TaskDetailView from "$lib/task/TaskDetail.svelte";
  import { workHref } from "$lib/work";

  let id = $derived(page.params.id ?? "");
  let detail = $state<TaskDetail | null>(null);
  let failure = $state<string | null>(null);
  let panelOpen = $state(true);
  // The Work list URL this view was opened from, kept across links to other
  // tasks so Escape brings back its filters.
  let from = $state<URL | null>(null);

  afterNavigate((navigation) => {
    if (navigation.from?.url.pathname === "/") from = navigation.from.url;
  });

  $effect(() => {
    const current = id;
    detail = null;
    failure = null;
    dataSource()
      .task(current)
      .then((loaded) => {
        if (current === id) detail = loaded;
      })
      .catch((error: unknown) => {
        if (current === id) failure = error instanceof Error ? error.message : String(error);
      });
  });

  function onkeydown(event: KeyboardEvent) {
    const action = taskKeyAction(event);
    if (action === null) return;
    if (action === "back") goto(workHref(id, from));
    else panelOpen = !panelOpen;
    event.preventDefault();
  }
</script>

<svelte:head><title>{detail?.task.title ?? id} · Arc Lightning</title></svelte:head>
<svelte:window {onkeydown} />

{#if failure !== null}
  <Feedback tone="danger" title="Could not load this task" message={failure} />
{:else if detail === null}
  <p class="loading" role="status">Loading task…</p>
{:else}
  <TaskDetailView {detail} workHref={workHref(id, from)} bind:panelOpen />
{/if}

<style>
  .loading { color: var(--arcl-text-muted); }
</style>
