<script lang="ts">
  import { Button, Feedback } from "@arclightning/ui";
  import { untrack } from "svelte";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import { dataSource, latestRead, type ChangeWarning, type TaskDetail } from "$lib/data";
  import { taskKeyAction } from "$lib/task";
  import TaskDetailView from "$lib/task/TaskDetail.svelte";
  import { workHref } from "$lib/work";

  let id = $derived(page.params.id ?? "");
  let detail = $state<TaskDetail | null>(null);
  let failure = $state<string | null>(null);
  let panelOpen = $state(true);
  /** Whether an open form holds input that leaving would lose. */
  let unsaved = $state(false);
  /** The warnings of the last saved change, shown until dismissed. */
  let warnings = $state<ChangeWarning[]>([]);
  // The Work list URL this view was opened from, kept across links to other
  // tasks so Escape brings back its filters.
  let from = $state<URL | null>(null);

  afterNavigate((navigation) => {
    if (navigation.from?.url.pathname === "/") from = navigation.from.url;
  });

  // The id the read in flight asks for. Each start aborts the read before it,
  // so a slow answer for a task left behind never replaces the one shown.
  let requested = "";
  const read = latestRead((signal) => dataSource().task(requested, signal), {
    loaded: (loaded) => (detail = loaded),
    failed: (error) => (failure = error instanceof Error ? error.message : String(error)),
  });

  $effect(() => {
    requested = id;
    // A navigation from a create carries its warnings.
    warnings = untrack(() => page.state?.warnings ?? []);
    load();
    return read.stop;
  });

  function load() {
    detail = null;
    failure = null;
    unsaved = false;
    read.start();
  }

  /**
   * Reload the task in place after a change. Unlike {@link load}, the view
   * stays mounted, so its notices about the change survive the reload.
   */
  async function refresh() {
    const current = id;
    const loaded = await dataSource().task(current);
    if (current === id) detail = loaded;
  }

  function onkeydown(event: KeyboardEvent) {
    const action = taskKeyAction(event, unsaved);
    if (action === null) return;
    if (action === "back") goto(workHref(id, from));
    else panelOpen = !panelOpen;
    event.preventDefault();
  }
</script>

<svelte:head><title>{detail === null ? "Task" : detail.task.title} · Arc Lightning</title></svelte:head>
<svelte:window {onkeydown} />

{#if failure !== null}
  <Feedback tone="danger" title="Could not load this task" message={failure}>
    {#snippet actions()}
      <Button label="Retry" onclick={load} />
    {/snippet}
  </Feedback>
{:else if detail === null}
  <p class="loading" role="status">Loading task…</p>
{:else}
  <TaskDetailView {detail} workHref={workHref(id, from)} bind:panelOpen bind:unsaved bind:warnings onchanged={refresh} />
{/if}

<style>
  .loading { color: var(--arcl-text-muted); }
</style>
