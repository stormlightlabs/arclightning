<script lang="ts">
  import { Feedback } from "@arclightning/ui";
  import { afterNavigate, goto } from "$app/navigation";
  import { page } from "$app/state";
  import { dataSource, type TaskDetail } from "$lib/data";
  import { useCommands } from "$lib/commands";
  import { isTaskAction, taskKeyAction, type TaskAction } from "$lib/task";
  import TaskDetailView from "$lib/task/TaskDetail.svelte";
  import { workHref } from "$lib/work";

  let id = $derived(page.params.id ?? "");
  let detail = $state<TaskDetail | null>(null);
  let failure = $state<string | null>(null);
  let panelOpen = $state(true);
  // The Work list URL this view was opened from, kept across links to other
  // tasks so Escape brings back its filters.
  let from = $state<URL | null>(null);

  // The Work list's menu asks for an action that takes text with `?action=`,
  // which applies to that task only.
  let asked = $state<{ id: string; action: TaskAction } | null>(null);

  afterNavigate((navigation) => {
    // The first load has a `from` without a URL.
    if (navigation.from?.url?.pathname === "/") from = navigation.from.url;
    const action = page.url.searchParams.get("action");
    if (asked !== null && asked.id !== id) asked = null;
    if (isTaskAction(action)) {
      asked = { id, action };
      const url = new URL(page.url);
      url.searchParams.delete("action");
      goto(url, { replaceState: true, keepFocus: true, noScroll: true });
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
      { id: "task-view:back", label: "Back to Work", scope: "view", group: "Task", keys: ["Escape"], viewKeys: true, icon: "arrowLeft", run: () => goto(workHref(id, from)) },
    ],
  }));

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

  /** Reload the task in place after a change, keeping the view's state. */
  async function refresh() {
    const current = id;
    const loaded = await dataSource().task(current);
    if (current === id) detail = loaded;
  }

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
  <TaskDetailView {detail} workHref={workHref(id, from)} bind:panelOpen onchanged={refresh} initialAction={asked?.id === id ? asked.action : null} />
{/if}

<style>
  .loading { color: var(--arcl-text-muted); }
</style>
