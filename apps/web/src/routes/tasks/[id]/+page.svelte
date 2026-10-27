<script lang="ts">
  import { Button, Feedback } from "@arclightning/ui";
  import { untrack } from "svelte";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import { dataSource, latestRead, type ChangeWarning, type TaskDetail } from "$lib/data";
  import { useCommands } from "$lib/commands";
  import { appUrl, appUrlHref } from "$lib/route";
  import { formAction, taskKeyAction, type TaskAction } from "$lib/task";
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

  // The Work list's menu opens the form of an action that takes text with
  // `?action=`, which applies to that task only. Any other value is ignored,
  // so a link never runs a change.
  let asked = $state<{ id: string; action: TaskAction } | null>(null);

  afterNavigate((navigation) => {
    // The first load has a `from` without a URL.
    const opener = navigation.from?.url ? appUrl(navigation.from.url) : null;
    if (opener?.pathname === "/") from = opener;
    if (asked !== null && asked.id !== id) asked = null;
    const url = appUrl(page.url);
    if (url.searchParams.has("action")) {
      const action = formAction(url.searchParams.get("action"));
      if (action !== null) asked = { id, action };
      url.searchParams.delete("action");
      goto(appUrlHref(url), { replaceState: true, keepFocus: true, noScroll: true });
    }
  });

  useCommands(() => ({
    commands: [
      {
        id: "task-view:properties",
        label: panelOpen ? "Collapse properties" : "Expand properties",
        scope: "view",
        group: "Task",
        keys: ["]"],
        viewKeys: true,
        run: () => (panelOpen = !panelOpen),
      },
      {
        id: "task-view:back",
        label: "Back to Work",
        scope: "view",
        group: "Task",
        keys: ["Escape"],
        viewKeys: true,
        icon: "arrowLeft",
        run: () => goto(workHref(id, from)),
      },
    ],
  }));

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
  <TaskDetailView
    {detail}
    workHref={workHref(id, from)}
    bind:panelOpen
    bind:unsaved
    bind:warnings
    onchanged={refresh}
    initialAction={asked?.id === id ? asked.action : null} />
{/if}

<style>
  .loading {
    color: var(--arcl-text-muted);
  }
</style>
