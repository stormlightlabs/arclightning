<script lang="ts">
  import {
    Button,
    EmptyState,
    Feedback,
    Field,
    MarkdownEditor,
    SelectMenu,
    TaskList,
    formatStatus,
    rowOrder,
    type SelectOption,
  } from "@arclightning/ui";
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { useCommands, type Command } from "$lib/commands";
  import {
    dataSource,
    latestRead,
    type ChangeWarning,
    type NewTask,
    type TaskPriority,
    type TaskStatus,
    type WorkList,
  } from "$lib/data";
  import { failureNotice } from "$lib/notice";
  import { appUrl, appUrlHref } from "$lib/route";
  import {
    ACTION_KEYS,
    ACTION_LABELS,
    ACTION_TEXT,
    actionsFor,
    failureOf,
    runAction,
    type ActionFailure,
    type TaskAction,
  } from "$lib/task";
  import WorkBoard from "$lib/work/WorkBoard.svelte";
  import {
    NO_MILESTONE,
    PRIORITIES,
    STATUSES,
    boardColumns,
    boardOrder,
    groupWork,
    isFiltered,
    moveSelection,
    orderMilestones,
    parseFilters,
    refusal,
    rememberView,
    selectedOf,
    taskHref,
    viewOf,
    visibleSelection,
    withKnownMilestone,
    workKeyAction,
    writeFilters,
    writeView,
    type BoardCard,
    type WorkFilters,
    type WorkView,
  } from "$lib/work";

  type FilterName = keyof WorkFilters;

  let list = $state<WorkList | null>(null);
  let failure = $state<string | null>(null);
  // The Task view's Escape comes back here with its task selected.
  let selectedId = $state<string | null>(selectedOf(appUrl(page.url).searchParams));

  /** The page's app path and query, which hold the filters. */
  let route = $derived(appUrl(page.url));
  let filters = $derived(parseFilters(route.searchParams));
  /** The layout: the URL's `view`, or the one this viewer last chose. */
  let view = $derived<WorkView>(viewOf(route.searchParams, () => window.localStorage));
  let groups = $derived(list ? groupWork(list, filters) : []);
  let columns = $derived(list && view === "board" ? boardColumns(list, filters) : []);
  let order = $derived(view === "board" ? boardOrder(columns) : rowOrder(groups));
  let board = $state<ReturnType<typeof WorkBoard>>();
  /** The card whose move is running. */
  let movingId = $state<string | null>(null);
  /** Why the last move the board refused did not run. */
  let refused = $state<string | null>(null);
  // A selected task the filters hide is not selected.
  let selected = $derived(visibleSelection(order, selectedId));

  const capitalize = (value: string) => value[0].toUpperCase() + value.slice(1);
  const any: SelectOption = { value: "", label: "Any" };
  let options = $derived<Record<FilterName, SelectOption[]>>({
    status: [any, ...STATUSES.map((status) => ({ value: status, label: formatStatus(status) }))],
    readiness: [
      any,
      { value: "ready", label: "Ready" },
      { value: "blocked", label: "Blocked" },
      { value: "waiting", label: "Waiting" },
    ],
    priority: [any, ...PRIORITIES.map((priority) => ({ value: priority, label: capitalize(priority) }))],
    milestone: [
      any,
      ...(list ? orderMilestones(list) : []).map((milestone) => ({ value: milestone.id, label: milestone.title })),
      { value: NO_MILESTONE, label: "No milestone" },
    ],
  });

  /** The New task form's fields, or `null` while it is closed. */
  let creating = $state<Required<Pick<NewTask, "title" | "body" | "priority">> | null>(null);
  let createBusy = $state(false);
  let createFailure = $state<ActionFailure | null>(null);

  /** The selected task's lifecycle action that last failed, shown above the list. */
  let actionFailure = $state<{ title: string; action: TaskAction; failure: ActionFailure } | null>(null);
  /** The warnings of the last action run from the list, shown until dismissed. */
  let actionWarnings = $state<ChangeWarning[]>([]);
  let actionBusy = $state(false);
  // Shortcuts and the menu act on the selected row only while the filters show it.
  let selectedTask = $derived(list?.items.find((item) => item.task.id === selected)?.task ?? null);

  const read = latestRead((signal) => dataSource().work(signal), {
    loaded: (loaded) => (list = loaded),
    failed: (error) => (failure = error instanceof Error ? error.message : String(error)),
  });

  onMount(() => {
    load();
    return read.stop;
  });

  // Once the list loads, and the router with it, keep the URL to the filters
  // the menus show, one value each and no milestone the list does not have,
  // and to the layout shown, so the URL names what the page shows.
  $effect(() => {
    if (list === null) return;
    const shown = withKnownMilestone(
      filters,
      list.milestones.map((milestone) => milestone.id),
    );
    const search = writeView(view, writeFilters(shown, route.searchParams));
    if (search.toString() !== route.searchParams.toString()) showSearch(search);
  });

  useCommands(() => ({
    context: selectedTask && { id: selectedTask.id, label: selectedTask.title },
    commands: [
      ...(selectedTask === null
        ? []
        : [
            {
              id: "work:open",
              label: "Open task",
              scope: "selection",
              keys: ["Enter"],
              viewKeys: true,
              icon: "arrowRight",
              run: () => goto(taskHref(selectedTask.id)),
            } satisfies Command,
            ...(actionBusy ? [] : actionsFor(selectedTask.status)).map((action): Command => {
              const id = selectedTask.id;
              return {
                id: `work:${action}`,
                label: ACTION_LABELS[action],
                scope: "selection",
                keys: [ACTION_KEYS[action]],
                icon: action === "complete" ? "check" : "task",
                run: () => act(id, action),
              };
            }),
          ]),
      ...(view === "board" && selectedTask !== null && !actionBusy
        ? [
            {
              id: "work:move",
              label: "Move",
              scope: "selection",
              keys: ["m"],
              icon: "arrowRight",
              run: () => board?.move(selected ?? ""),
            } satisfies Command,
          ]
        : []),
      {
        id: "work:view",
        label: view === "board" ? "Show as list" : "Show as board",
        scope: "view",
        group: "Work",
        keys: ["v"],
        run: () => setView(view === "board" ? "list" : "board"),
      },
      {
        id: "work:new",
        label: "New task",
        scope: "view",
        group: "Work",
        keys: ["n"],
        icon: "capture",
        run: openCreate,
      },
      {
        id: "work:next",
        label: "Select the next task",
        scope: "view",
        group: "Work",
        keys: ["j"],
        viewKeys: true,
        run: () => (selectedId = moveSelection(order, selectedId, 1)),
      },
      {
        id: "work:previous",
        label: "Select the previous task",
        scope: "view",
        group: "Work",
        keys: ["k"],
        viewKeys: true,
        run: () => (selectedId = moveSelection(order, selectedId, -1)),
      },
      ...(isFiltered(filters)
        ? [
            {
              id: "work:clear",
              label: "Clear filters",
              scope: "view",
              group: "Work",
              run: () => showFilters({ status: [], readiness: [], priority: [], milestone: [] }),
            } satisfies Command,
          ]
        : []),
    ],
  }));

  /**
   * Run `action` on task `id` as the Task view's button does. An action that
   * takes Markdown opens the Task view with its form instead.
   */
  async function act(id: string, action: TaskAction) {
    if (actionBusy) return;
    if (ACTION_TEXT[action] !== undefined) {
      await goto(`${taskHref(id)}?action=${action}`);
      return;
    }
    const title = list?.items.find((item) => item.task.id === id)?.task.title ?? id;
    actionFailure = null;
    actionWarnings = [];
    refused = null;
    actionBusy = true;
    try {
      actionWarnings = (await runAction(dataSource(), id, action)).warnings;
    } catch (error) {
      actionFailure = { title, action, failure: failureOf(error) };
      return;
    } finally {
      actionBusy = false;
    }
    // Reload in place, so the list and its selection stay on screen.
    read.start();
  }

  /**
   * Run the move the board asked for, as the list's action does. The card
   * stays in its column until the reload after the change shows it moved; a
   * failed or queued move leaves it there. While a change runs, another
   * does not start.
   */
  async function move(card: BoardCard, action: TaskAction, text: string) {
    if (actionBusy) return;
    actionFailure = null;
    actionWarnings = [];
    refused = null;
    actionBusy = true;
    movingId = card.id;
    try {
      actionWarnings = (await runAction(dataSource(), card.id, action, text)).warnings;
    } catch (error) {
      actionFailure = { title: card.title, action, failure: failureOf(error) };
      return;
    } finally {
      actionBusy = false;
      movingId = null;
    }
    read.start();
  }

  function refuse(card: BoardCard, to: TaskStatus) {
    actionFailure = null;
    refused = `${card.title}: ${refusal(card.status, to)}`;
  }

  function setView(next: WorkView) {
    rememberView(() => window.localStorage, next);
    showSearch(writeView(next, route.searchParams));
  }

  function openCreate() {
    creating = { title: "", body: "", priority: "normal" };
    createFailure = null;
  }

  /** Create the task as `arcl task create` does, then open it. */
  async function create(event: SubmitEvent) {
    event.preventDefault();
    if (creating === null) return;
    const { title, body, priority } = creating;
    createBusy = true;
    createFailure = null;
    try {
      const { value: task, warnings } = await dataSource().createTask(
        body === "" ? { title, priority } : { title, body, priority },
      );
      creating = null;
      // The Task view shows the create's warnings.
      await goto(taskHref(task.id), { state: { warnings } });
    } catch (error) {
      createFailure = failureOf(error);
    } finally {
      createBusy = false;
    }
  }

  function load() {
    failure = null;
    list = null;
    read.start();
  }

  function setFilter(name: FilterName, value: string) {
    const next = { ...filters, [name]: value === "" ? [] : [value] };
    showFilters(next);
  }

  function showFilters(next: WorkFilters) {
    showSearch(writeFilters(next, appUrl(page.url).searchParams));
  }

  function showSearch(search: URLSearchParams) {
    const url = appUrl(page.url);
    url.search = search.toString();
    goto(appUrlHref(url), { replaceState: true, keepFocus: true, noScroll: true });
  }

  function onkeydown(event: KeyboardEvent) {
    const action = workKeyAction(event);
    if (action === null) return;
    if (action === "open") {
      if (selected === null) return;
      goto(taskHref(selected));
    } else {
      selectedId = moveSelection(order, selected, action === "next" ? 1 : -1);
    }
    event.preventDefault();
  }
</script>

<svelte:head><title>Work · Arc Lightning</title></svelte:head>
<svelte:window {onkeydown} />

<header>
  <div class="title">
    <h1>Work</h1>
    {#if creating === null}<Button label="New task" variant="primary" onclick={openCreate} />{/if}
    <div class="layout" role="group" aria-label="Layout">
      <button type="button" aria-pressed={view === "list"} onclick={() => setView("list")}>List</button>
      <button type="button" aria-pressed={view === "board"} onclick={() => setView("board")}>Board</button>
    </div>
  </div>
  <form class="filters" aria-label="Filters" onsubmit={(event) => event.preventDefault()}>
    {#each [["status", "Status"], ["readiness", "Readiness"], ["priority", "Priority"], ["milestone", "Milestone"]] as const as [name, label] (name)}
      <SelectMenu
        {label}
        options={options[name]}
        value={filters[name][0] ?? ""}
        onchange={(value) => setFilter(name, value)} />
    {/each}
  </form>
</header>

{#if creating !== null}
  <form class="create" aria-labelledby="create-title" onsubmit={create}>
    <h2 id="create-title">New task</h2>
    <Field label="Title" bind:value={creating.title} required />
    <SelectMenu
      label="Priority"
      options={options.priority.slice(1)}
      value={creating.priority}
      onchange={(value) => creating && (creating.priority = value as TaskPriority)} />
    <span class="label">Description</span>
    <MarkdownEditor
      value={creating.body}
      label="Description Markdown"
      readonly={false}
      onchange={(value) => creating && (creating.body = value)} />
    <div class="buttons">
      <Button type="submit" variant="primary" label="Create" busy={createBusy} disabled={createBusy} />
      <Button label="Cancel" variant="ghost" disabled={createBusy} onclick={() => (creating = null)} />
    </div>
    {#if createFailure !== null}
      <Feedback {...failureNotice(createFailure, "Could not create the task")} />
    {/if}
  </form>
{/if}

{#if refused !== null}
  <Feedback tone="danger" title="Could not move the task" message={refused}>
    {#snippet actions()}
      <Button label="Dismiss" variant="ghost" onclick={() => (refused = null)} />
    {/snippet}
  </Feedback>
{/if}
{#if actionFailure !== null}
  <Feedback
    {...failureNotice(
      actionFailure.failure,
      `Could not ${ACTION_LABELS[actionFailure.action].toLowerCase()} ${actionFailure.title}`,
    )} />
{/if}
{#each actionWarnings as warning, index (index)}
  <Feedback tone="warning" title="The change is saved, with a warning" message={`${warning.message} (${warning.code})`}>
    {#snippet actions()}
      <Button
        label="Dismiss"
        variant="ghost"
        onclick={() => (actionWarnings = actionWarnings.filter((_, other) => other !== index))} />
    {/snippet}
  </Feedback>
{/each}

{#if failure !== null}
  <Feedback tone="danger" title="Could not load tasks" message={failure}>
    {#snippet actions()}
      <Button label="Retry" onclick={load} />
    {/snippet}
  </Feedback>
{:else if list === null}
  <p class="loading" role="status">Loading tasks…</p>
{:else if list.items.length === 0}
  <EmptyState
    title="No tasks yet"
    message="Tasks appear here once you create one with New task or arcl task create, or apply a plan." />
{:else if order.length === 0}
  <EmptyState title="No tasks match these filters" message="Change or clear the filters to see more tasks.">
    {#snippet actions()}
      <Button
        label="Clear filters"
        onclick={() => showFilters({ status: [], readiness: [], priority: [], milestone: [] })} />
    {/snippet}
  </EmptyState>
{:else if view === "board"}
  <WorkBoard
    bind:this={board}
    {columns}
    selectedId={selected}
    busyId={movingId}
    locked={actionBusy}
    onselect={(id) => (selectedId = id)}
    onmove={move}
    onrefuse={refuse} />
{:else}
  <TaskList {groups} href={taskHref} selectedId={selected} onselect={(id) => (selectedId = id)} label="Work" />
{/if}

<style>
  header {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    justify-content: space-between;
    gap: var(--arcl-space-4);
    margin-bottom: var(--arcl-space-4);
  }
  .title {
    display: flex;
    align-items: center;
    gap: var(--arcl-space-3);
  }
  .create {
    display: grid;
    gap: var(--arcl-space-3);
    max-width: 48rem;
    margin-bottom: var(--arcl-space-5);
    padding: var(--arcl-space-4);
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-md);
    background: var(--arcl-surface);
  }
  .create h2 {
    margin: 0;
    color: var(--arcl-heading);
    font: 600 var(--arcl-type-base) / 1.3 var(--arcl-font-ui);
  }
  .label {
    color: var(--arcl-text-muted);
    font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui);
  }
  .buttons {
    display: flex;
    gap: var(--arcl-space-2);
  }
  h1 {
    margin: 0;
    color: var(--arcl-heading);
    font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui);
  }
  .layout {
    display: inline-flex;
    overflow: hidden;
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-control);
  }
  .layout button {
    min-height: 2.25rem;
    padding-inline: var(--arcl-space-3);
    border: 0;
    color: var(--arcl-text-muted);
    background: var(--arcl-surface);
    font: 600 var(--arcl-type-sm) var(--arcl-font-ui);
    cursor: pointer;
  }
  .layout button + button {
    border-inline-start: var(--arcl-border-width) solid var(--arcl-border);
  }
  .layout button:hover {
    background: var(--arcl-surface-hover);
  }
  .layout button[aria-pressed="true"] {
    color: var(--arcl-heading);
    background: var(--arcl-surface-subtle);
    box-shadow: inset 0 -2px 0 var(--arcl-link);
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: var(--arcl-space-3);
    font-size: var(--arcl-type-sm);
  }
  .loading {
    color: var(--arcl-text-muted);
  }
</style>
