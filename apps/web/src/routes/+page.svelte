<script lang="ts">
  import { Button, EmptyState, Feedback, Select, TaskList, formatStatus, rowOrder, type SelectOption } from "@arclightning/ui";
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import { dataSource, type WorkList } from "$lib/data";
  import {
    NO_MILESTONE,
    PRIORITIES,
    STATUSES,
    groupWork,
    isFiltered,
    moveSelection,
    parseFilters,
    selectedOf,
    taskHref,
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

  const capitalize = (value: string) => value[0].toUpperCase() + value.slice(1);
  const any: SelectOption = { value: "", label: "Any" };
  let options = $derived<Record<FilterName, SelectOption[]>>({
    status: [any, ...STATUSES.map((status) => ({ value: status, label: formatStatus(status) }))],
    readiness: [any, { value: "ready", label: "Ready" }, { value: "blocked", label: "Blocked" }, { value: "waiting", label: "Waiting" }],
    priority: [any, ...PRIORITIES.map((priority) => ({ value: priority, label: capitalize(priority) }))],
    milestone: [
      any,
      ...(list?.milestones ?? []).map((milestone) => ({ value: milestone.id, label: milestone.title })),
      { value: NO_MILESTONE, label: "No milestone" },
    ],
  });

  onMount(() => {
    dataSource()
      .work()
      .then((loaded) => (list = loaded))
      .catch((error: unknown) => (failure = error instanceof Error ? error.message : String(error)));
  });

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
      if (selectedId === null) return;
      goto(taskHref(selectedId));
    } else {
      selectedId = moveSelection(order, selectedId, action === "next" ? 1 : -1);
    }
    event.preventDefault();
  }
</script>

<svelte:head><title>Work · Arc Lightning</title></svelte:head>
<svelte:window {onkeydown} />

<header>
  <h1>Work</h1>
  <form class="filters" aria-label="Filters" onsubmit={(event) => event.preventDefault()}>
    {#each [["status", "Status"], ["readiness", "Readiness"], ["priority", "Priority"], ["milestone", "Milestone"]] as const as [name, label] (name)}
      <Select {label} options={options[name]} value={filters[name][0] ?? ""} onchange={(event) => setFilter(name, event.currentTarget.value)} />
    {/each}
  </form>
</header>

{#if failure !== null}
  <Feedback tone="danger" title="Could not load tasks" message={failure} />
{:else if list === null}
  <p class="loading" role="status">Loading tasks…</p>
{:else if list.items.length === 0}
  <EmptyState title="No tasks yet" message="Tasks appear here once you create them with arcl task create or apply a plan." />
{:else if groups.length === 0}
  <EmptyState title="No tasks match these filters" message="Change or clear the filters to see more tasks.">
    {#snippet actions()}
      <Button label="Clear filters" onclick={() => showFilters({ status: [], readiness: [], priority: [], milestone: [] })} />
    {/snippet}
  </EmptyState>
{:else}
  <TaskList {groups} href={taskHref} {selectedId} onselect={(id) => (selectedId = id)} label="Work" />
{/if}

<style>
  header { display: flex; flex-wrap: wrap; align-items: end; justify-content: space-between; gap: var(--arcl-space-4); margin-bottom: var(--arcl-space-4); }
  h1 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui); }
  .filters { display: flex; flex-wrap: wrap; gap: var(--arcl-space-3); font-size: var(--arcl-type-sm); }
  .loading { color: var(--arcl-text-muted); }
</style>
