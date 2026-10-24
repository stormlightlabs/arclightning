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
  import { dataSource, latestRead, type NewTask, type TaskPriority, type WorkList } from "$lib/data";
  import { failureOf, type ActionFailure } from "$lib/task";
  import {
    NO_MILESTONE,
    PRIORITIES,
    STATUSES,
    groupWork,
    isFiltered,
    moveSelection,
    orderMilestones,
    parseFilters,
    selectedOf,
    taskHref,
    visibleSelection,
    withKnownMilestone,
    workKeyAction,
    writeFilters,
    type WorkFilters,
  } from "$lib/work";

  type FilterName = keyof WorkFilters;

  let list = $state<WorkList | null>(null);
  let failure = $state<string | null>(null);
  // The Task view's Escape comes back here with its task selected.
  let selectedId = $state<string | null>(selectedOf(page.url.searchParams));

  let filters = $derived(parseFilters(page.url.searchParams));
  let groups = $derived(list ? groupWork(list, filters) : []);
  let order = $derived(rowOrder(groups));
  // A selected task the filters hide is not selected.
  let selected = $derived(visibleSelection(order, selectedId));

  const capitalize = (value: string) => value[0].toUpperCase() + value.slice(1);
  const any: SelectOption = { value: "", label: "Any" };
  let options = $derived<Record<FilterName, SelectOption[]>>({
    status: [any, ...STATUSES.map((status) => ({ value: status, label: formatStatus(status) }))],
    readiness: [any, { value: "ready", label: "Ready" }, { value: "blocked", label: "Blocked" }, { value: "waiting", label: "Waiting" }],
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

  const read = latestRead((signal) => dataSource().work(signal), {
    loaded: (loaded) => (list = loaded),
    failed: (error) => (failure = error instanceof Error ? error.message : String(error)),
  });

  onMount(() => {
    load();
    return read.stop;
  });

  // Once the list loads, and the router with it, keep the URL to the filters
  // the menus show: one value each, and no milestone the list does not have.
  $effect(() => {
    if (list === null) return;
    const shown = withKnownMilestone(filters, list.milestones.map((milestone) => milestone.id));
    if (writeFilters(shown, page.url.searchParams).toString() !== page.url.searchParams.toString()) showFilters(shown);
  });

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
      const task = await dataSource().createTask(body === "" ? { title, priority } : { title, body, priority });
      creating = null;
      await goto(taskHref(task.id));
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
    const url = new URL(page.url);
    url.search = writeFilters(next, url.searchParams).toString();
    goto(url, { replaceState: true, keepFocus: true, noScroll: true });
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
  </div>
  <form class="filters" aria-label="Filters" onsubmit={(event) => event.preventDefault()}>
    {#each [["status", "Status"], ["readiness", "Readiness"], ["priority", "Priority"], ["milestone", "Milestone"]] as const as [name, label] (name)}
      <SelectMenu {label} options={options[name]} value={filters[name][0] ?? ""} onchange={(value) => setFilter(name, value)} />
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
      onchange={(value) => creating && (creating.priority = value as TaskPriority)}
    />
    <span class="label">Description</span>
    <MarkdownEditor value={creating.body} label="Description Markdown" readonly={false} onchange={(value) => creating && (creating.body = value)} />
    <div class="buttons">
      <Button type="submit" variant="primary" label="Create" busy={createBusy} disabled={createBusy} />
      <Button label="Cancel" variant="ghost" disabled={createBusy} onclick={() => (creating = null)} />
    </div>
    {#if createFailure !== null}
      <Feedback tone="danger" title="Could not create the task" message={`${createFailure.message} (${createFailure.code})`} />
    {/if}
  </form>
{/if}

{#if failure !== null}
  <Feedback tone="danger" title="Could not load tasks" message={failure}>
    {#snippet actions()}
      <Button label="Retry" onclick={load} />
    {/snippet}
  </Feedback>
{:else if list === null}
  <p class="loading" role="status">Loading tasks…</p>
{:else if list.items.length === 0}
  <EmptyState title="No tasks yet" message="Tasks appear here once you create one with New task or arcl task create, or apply a plan." />
{:else if groups.length === 0}
  <EmptyState title="No tasks match these filters" message="Change or clear the filters to see more tasks.">
    {#snippet actions()}
      <Button label="Clear filters" onclick={() => showFilters({ status: [], readiness: [], priority: [], milestone: [] })} />
    {/snippet}
  </EmptyState>
{:else}
  <TaskList {groups} href={taskHref} selectedId={selected} onselect={(id) => (selectedId = id)} label="Work" />
{/if}

<style>
  header { display: flex; flex-wrap: wrap; align-items: end; justify-content: space-between; gap: var(--arcl-space-4); margin-bottom: var(--arcl-space-4); }
  .title { display: flex; align-items: center; gap: var(--arcl-space-3); }
  .create { display: grid; gap: var(--arcl-space-3); max-width: 48rem; margin-bottom: var(--arcl-space-5); padding: var(--arcl-space-4); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); }
  .create h2 { margin: 0; color: var(--arcl-heading); font: 600 var(--arcl-type-base) / 1.3 var(--arcl-font-ui); }
  .label { color: var(--arcl-text-muted); font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui); }
  .buttons { display: flex; gap: var(--arcl-space-2); }
  h1 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui); }
  .filters { display: flex; flex-wrap: wrap; gap: var(--arcl-space-3); font-size: var(--arcl-type-sm); }
  .loading { color: var(--arcl-text-muted); }
</style>
