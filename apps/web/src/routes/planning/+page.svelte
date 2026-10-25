<script lang="ts">
  import { Badge, Button, EmptyState, Feedback, Field, PlanningTree, type BadgeTone } from "@arclightning/ui";
  import { onMount, tick } from "svelte";
  import { SvelteSet } from "svelte/reactivity";
  import { goto } from "$app/navigation";
  import { useCommands, type Command } from "$lib/commands";
  import { dataSource, latestRead, type ChangeWarning, type Planning } from "$lib/data";
  import {
    PLANNING_KEYS,
    closedRecords,
    describeProgress,
    planTemplate,
    planningView,
    visibleRows,
    type MilestoneRow,
    type PlanningRow,
  } from "$lib/planning";
  import { failureNotice } from "$lib/notice";
  import { failureOf, type ActionFailure } from "$lib/task";
  import { moveSelection, taskHref, visibleSelection, workKeyAction } from "$lib/work";

  /** A form the view shows, with the record it acts on. */
  type PlanningForm =
    | { kind: "spec"; title: string; body: string; acceptance: string }
    | { kind: "plan"; spec: string; title: string; body: string }
    | { kind: "apply"; plan: string; document: string };

  let planning = $state<Planning | null>(null);
  let failure = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  /** Milestones whose task tree is open. */
  const expanded = new SvelteSet<string>();
  /** Specs and plans whose rows are hidden; closed ones start here. */
  const collapsed = new SvelteSet<string>();
  /** Whether the closed records have been collapsed, which happens on the first load only. */
  let collapsedClosed = false;

  let specs = $derived(planning ? planningView(planning) : []);
  let rows = $derived(visibleRows(specs, collapsed));
  let order = $derived(rows.map((row) => row.id));
  let selected = $derived(visibleSelection(order, selectedId));
  let selectedRow = $derived(rows.find((row) => row.id === selected) ?? null);

  let form = $state<PlanningForm | null>(null);
  /** The open form as it opened, to tell whether it holds edits. */
  let opened = "";
  /** Whether the open form holds edits, which Escape and the form keys keep. */
  let unsaved = $derived(form !== null && JSON.stringify(form) !== opened);
  let busy = $state(false);
  let formFailure = $state<ActionFailure | null>(null);
  let notice = $state<string | null>(null);
  let warnings = $state<ChangeWarning[]>([]);
  let root = $state<HTMLElement>();

  const read = latestRead((signal) => dataSource().planning(signal), {
    loaded: (loaded) => {
      planning = loaded;
      if (!collapsedClosed) {
        collapsedClosed = true;
        for (const id of closedRecords(planningView(loaded))) collapsed.add(id);
      }
    },
    failed: (error) => (failure = error instanceof Error ? error.message : String(error)),
  });

  onMount(() => {
    load();
    return read.stop;
  });

  // Keep focus on the selected row while focus is in the view or nowhere.
  $effect(() => {
    const id = selected;
    if (id === null || !root) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && !root.contains(active)) return;
    if (active !== null && active.closest("form")) return;
    root.querySelector<HTMLElement>(`[data-row-id="${CSS.escape(id)}"]`)?.focus();
  });

  useCommands(() => ({
    context: selectedRow && { id: selectedRow.id, label: selectedRow.title },
    commands: [
      ...(selectedRow === null ? [] : selectionCommands(selectedRow)),
      { id: "planning:new-spec", label: "New spec", scope: "view", group: "Planning", keys: [PLANNING_KEYS.newSpec], icon: "spec", run: () => openForm({ kind: "spec", title: "", body: "", acceptance: "" }) },
      { id: "planning:next", label: "Select the next row", scope: "view", group: "Planning", keys: ["j"], viewKeys: true, run: () => move(1) },
      { id: "planning:previous", label: "Select the previous row", scope: "view", group: "Planning", keys: ["k"], viewKeys: true, run: () => move(-1) },
    ],
  }));

  function selectionCommands(row: PlanningRow): Command[] {
    const open = isOpen(row);
    const toggleLabel = row.kind === "milestone" ? (open ? "Hide tasks" : "Show tasks") : open ? "Collapse" : "Expand";
    const commands: Command[] = [{ id: "planning:toggle", label: toggleLabel, scope: "selection", keys: ["Enter"], viewKeys: true, run: () => toggle(row) }];
    if (row.kind === "spec") {
      commands.push({ id: "planning:new-plan", label: "New plan", scope: "selection", keys: [PLANNING_KEYS.add], icon: "plan", run: () => openForm({ kind: "plan", spec: row.id, title: "", body: "" }) });
    }
    if (row.kind === "plan") {
      commands.push({ id: "planning:apply", label: "Apply plan", scope: "selection", keys: [PLANNING_KEYS.add], icon: "plan", run: () => openForm({ kind: "apply", plan: row.id, document: planTemplate() }) });
    }
    return commands;
  }

  function load() {
    failure = null;
    planning = null;
    read.start();
  }

  function move(step: 1 | -1) {
    selectedId = moveSelection(order, selected, step);
  }

  function isOpen(row: PlanningRow): boolean {
    return row.kind === "milestone" ? expanded.has(row.id) : !collapsed.has(row.id);
  }

  function toggle(row: PlanningRow) {
    selectedId = row.id;
    const set = row.kind === "milestone" ? expanded : collapsed;
    if (set.has(row.id)) set.delete(row.id);
    else set.add(row.id);
  }

  async function openForm(next: PlanningForm) {
    // A form with edits, or one whose request is in flight, stays open; the
    // key takes focus back to it.
    if (!unsaved && !busy) {
      opened = JSON.stringify(next);
      form = next;
      formFailure = null;
      notice = null;
      await tick();
    }
    const element = document.querySelector<HTMLElement>("form.planning-form");
    if (element && !element.contains(document.activeElement)) element.querySelector<HTMLElement>("input, textarea")?.focus();
  }

  function closeForm() {
    const owner = form?.kind === "plan" ? form.spec : form?.kind === "apply" ? form.plan : null;
    form = null;
    formFailure = null;
    if (owner !== null) selectedId = owner;
  }

  /** Run the open form's change, then reload in place and select what it made. */
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (form === null || busy) return;
    const current = form;
    busy = true;
    formFailure = null;
    try {
      const source = dataSource();
      if (current.kind === "spec") {
        const { value, warnings: given } = await source.createSpec({
          title: current.title,
          ...(current.body.trim() === "" ? {} : { body: current.body }),
          ...(current.acceptance.trim() === "" ? {} : { acceptance_criteria: current.acceptance }),
        });
        warnings = given;
        notice = `Created the spec “${value.title}”. Press ${PLANNING_KEYS.add.toUpperCase()} on it to add a plan.`;
        selectedId = value.id;
      } else if (current.kind === "plan") {
        const { value, warnings: given } = await source.createPlan({
          title: current.title,
          spec: current.spec,
          ...(current.body.trim() === "" ? {} : { body: current.body }),
        });
        warnings = given;
        notice = `Created the plan “${value.title}”. Press ${PLANNING_KEYS.add.toUpperCase()} on it to apply phases and tasks.`;
        collapsed.delete(current.spec);
        selectedId = value.id;
      } else {
        const { value, warnings: given } = await source.applyPlan(current.plan, current.document);
        warnings = given;
        notice = `Applied “${value.plan.title}”: it has ${count(value.phases.length, "phase")} and ${count(value.tasks.length, "task")}.`;
        collapsed.delete(current.plan);
        for (const phase of value.phases) expanded.add(phase.id);
        selectedId = value.plan.id;
      }
      // Only the form that sent the request closes, so a newer one keeps its edits.
      if (form === current) form = null;
      read.start();
    } catch (error) {
      if (form === current) formFailure = failureOf(error);
    } finally {
      busy = false;
    }
  }

  function count(value: number, noun: string): string {
    return value === 1 ? `1 ${noun}` : `${value} ${noun}s`;
  }

  const tones: Record<string, BadgeTone> = { open: "info", completed: "success", cancelled: "danger" };

  function statusLabel(status: string): string {
    return status[0].toUpperCase() + status.slice(1).replaceAll("_", " ");
  }

  function readiness(row: MilestoneRow): string {
    const parts = [];
    if (row.ready) parts.push(`${row.ready} ready`);
    if (row.blocked) parts.push(`${row.blocked} blocked`);
    return parts.join(" · ") || (row.status === "open" ? "Nothing ready" : "");
  }

  function onkeydown(event: KeyboardEvent) {
    // Escape closes the form, unless it holds edits, its request is in flight,
    // or an open listbox or a dialog took it.
    if (event.key === "Escape" && form !== null && !unsaved && !busy && !event.defaultPrevented) {
      closeForm();
      event.preventDefault();
      return;
    }
    // A held Enter or Space on a row clicks it once, not on each repeat.
    if (event.repeat && (event.key === "Enter" || event.key === " ") && event.target instanceof Element && event.target.closest("[data-row-id]")) {
      event.preventDefault();
      return;
    }
    const action = workKeyAction(event);
    if (action === null) return;
    if (action === "open") {
      // A held Enter toggles once.
      if (selectedRow === null || event.repeat) return;
      toggle(selectedRow);
    } else {
      move(action === "next" ? 1 : -1);
    }
    event.preventDefault();
  }
</script>

<svelte:head><title>Planning · Arc Lightning</title></svelte:head>
<svelte:window {onkeydown} />

{#snippet formFor(current: PlanningForm)}
  <form class="planning-form" aria-labelledby="planning-form-title" onsubmit={submit}>
    {#if current.kind === "spec"}
      <h2 id="planning-form-title">New spec</h2>
      <Field label="Title" bind:value={current.title} required />
      <Field label="Description" description="Markdown." multiline bind:value={current.body} />
      <Field label="Acceptance criteria" description="Markdown." multiline bind:value={current.acceptance} />
    {:else if current.kind === "plan"}
      <h2 id="planning-form-title">New plan</h2>
      <Field label="Title" bind:value={current.title} required />
      <Field label="Description" description="Markdown." multiline bind:value={current.body} />
    {:else}
      <h2 id="planning-form-title">Apply plan</h2>
      <Field
        label="Plan document"
        description="TOML, as arcl plan apply --file reads it. Applying again with the same keys updates the phases and tasks."
        multiline
        spellcheck={false}
        bind:value={current.document}
      />
    {/if}
    <div class="buttons">
      <Button type="submit" variant="primary" label={current.kind === "apply" ? "Apply" : "Create"} {busy} disabled={busy} />
      <Button label="Cancel" variant="ghost" disabled={busy} onclick={closeForm} />
    </div>
    {#if formFailure !== null}
      <Feedback {...failureNotice(formFailure, "Could not save this")} />
    {/if}
  </form>
{/snippet}

<header>
  <h1>Planning</h1>
  <Button label="New spec" variant="primary" onclick={() => openForm({ kind: "spec", title: "", body: "", acceptance: "" })} />
</header>

{#if notice !== null}<p class="notice" role="status">{notice}</p>{/if}
{#each warnings as warning, index (index)}
  <Feedback tone="warning" title="The change is saved, with a warning" message={`${warning.message} (${warning.code})`}>
    {#snippet actions()}
      <Button label="Dismiss" variant="ghost" onclick={() => (warnings = warnings.filter((_, other) => other !== index))} />
    {/snippet}
  </Feedback>
{/each}
{#if form?.kind === "spec"}{@render formFor(form)}{/if}

{#if failure !== null}
  <Feedback tone="danger" title="Could not load Planning" message={failure}>
    {#snippet actions()}
      <Button label="Retry" onclick={load} />
    {/snippet}
  </Feedback>
{:else if planning === null}
  <p class="muted" role="status">Loading plans…</p>
{:else if specs.length === 0}
  <EmptyState title="No specs yet" message="Create a spec with New spec, or promote a capture to one from the Inbox. Its plans and milestones appear here." />
{:else}
  <div class="planning" bind:this={root}>
    {#each specs as spec (spec.id)}
      <section class="spec" aria-label={spec.title}>
        <div class="head" data-selected={selected === spec.id || undefined}>
          <button
            type="button"
            class="toggle spec-title"
            data-row-id={spec.id}
            aria-expanded={!collapsed.has(spec.id)}
            onfocus={() => (selectedId = spec.id)}
            onclick={() => toggle(spec)}
          >
            <span class="chevron" aria-hidden="true">{collapsed.has(spec.id) ? "▸" : "▾"}</span>
            <span class="title">{spec.title}</span>
            <span class="detail">Spec · {count(spec.plans.length, "plan")}</span>
          </button>
          <Badge label={statusLabel(spec.status)} tone={tones[spec.status] ?? "neutral"} />
          <Button label="New plan" variant="ghost" aria-keyshortcuts="A" onclick={() => openForm({ kind: "plan", spec: spec.id, title: "", body: "" })} />
        </div>
        {#if form?.kind === "plan" && form.spec === spec.id}{@render formFor(form)}{/if}
        {#if !collapsed.has(spec.id)}
          {#if spec.plans.length === 0}
            <p class="muted empty">No plans yet. Add one with New plan.</p>
          {/if}
          {#each spec.plans as plan (plan.id)}
            <div class="head plan" data-selected={selected === plan.id || undefined}>
              <button
                type="button"
                class="toggle"
                data-row-id={plan.id}
                aria-expanded={!collapsed.has(plan.id)}
                onfocus={() => (selectedId = plan.id)}
                onclick={() => toggle(plan)}
              >
                <span class="chevron" aria-hidden="true">{collapsed.has(plan.id) ? "▸" : "▾"}</span>
                <span class="title">{plan.title}</span>
                <span class="detail">Plan · {describeProgress(plan)}</span>
              </button>
              <Badge label={statusLabel(plan.status)} tone={tones[plan.status] ?? "neutral"} />
              <Button label="Apply plan" variant="ghost" aria-keyshortcuts="A" onclick={() => openForm({ kind: "apply", plan: plan.id, document: planTemplate() })} />
            </div>
            {#if form?.kind === "apply" && form.plan === plan.id}{@render formFor(form)}{/if}
            {#if !collapsed.has(plan.id)}
              {#if plan.milestones.length === 0}
                <p class="muted empty">No milestones yet. Apply a plan document to add phases and tasks.</p>
              {:else}
                <ul class="milestones" aria-label={`Milestones of ${plan.title}`}>
                  {#each plan.milestones as row (row.id)}
                    <li>
                      <button
                        type="button"
                        class="milestone"
                        data-row-id={row.id}
                        data-selected={selected === row.id || undefined}
                        data-done={row.status !== "open" || undefined}
                        aria-expanded={expanded.has(row.id)}
                        onfocus={() => (selectedId = row.id)}
                        onclick={() => toggle(row)}
                      >
                        <svg class="ring" width="18" height="18" viewBox="0 0 20 20" role="img" aria-label={row.status === "completed" ? "Done" : row.status === "cancelled" ? "Cancelled" : "Open"}>
                          {#if row.status === "completed"}
                            <circle cx="10" cy="10" r="9" fill="currentColor" />
                            <path d="m6.2 10.2 2.6 2.6 5-5.3" fill="none" stroke="var(--arcl-surface)" stroke-width="1.8" />
                          {:else}
                            <circle cx="10" cy="10" r="8" fill="none" stroke="currentColor" stroke-width="2" />
                          {/if}
                        </svg>
                        <span class="title">{row.title}</span>
                        <span class="progress">
                          <span class="bar" aria-hidden="true"><span style:width={`${row.total ? (100 * row.done) / row.total : 0}%`}></span></span>
                          <span class="detail">{describeProgress(row)}</span>
                        </span>
                        <span class="now">
                          {#if row.ready}<span class="ready">{row.ready} ready</span>{/if}
                          {#if row.ready && row.blocked} · {/if}
                          {#if row.blocked}<span class="blocked">{row.blocked} blocked</span>{/if}
                          {#if !row.ready && !row.blocked}<span class="detail">{readiness(row)}</span>{/if}
                        </span>
                      </button>
                      {#if expanded.has(row.id)}
                        <div class="tree">
                          {#if row.tree.length}
                            <PlanningTree label={`Tasks in ${row.title}`} nodes={row.tree} onselect={(node) => goto(taskHref(node.id))} />
                          {:else}
                            <p class="muted">No tasks in this milestone.</p>
                          {/if}
                        </div>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if plan.tasks.length}
                <div class="tree loose">
                  <p class="muted">Outside a milestone</p>
                  <PlanningTree label={`Tasks in ${plan.title} outside a milestone`} nodes={plan.tasks} onselect={(node) => goto(taskHref(node.id))} />
                </div>
              {/if}
            {/if}
          {/each}
        {/if}
      </section>
    {/each}
  </div>
{/if}

<style>
  header { display: flex; flex-wrap: wrap; align-items: center; gap: var(--arcl-space-3); margin-bottom: var(--arcl-space-4); }
  h1 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui); }
  .muted { margin: 0; color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); }
  .notice { margin: 0 0 var(--arcl-space-4); padding: var(--arcl-space-2) var(--arcl-space-3); border-inline-start: 4px solid var(--arcl-success); border-radius: var(--arcl-radius-sm); background: var(--arcl-success-surface); font-size: var(--arcl-type-sm); }
  .planning { display: grid; gap: var(--arcl-space-4); }
  .spec { overflow: hidden; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); }
  .head { display: flex; flex-wrap: wrap; align-items: center; gap: var(--arcl-space-2); padding: var(--arcl-space-1) var(--arcl-space-3); }
  .head.plan { padding-inline-start: var(--arcl-space-6); border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); background: var(--arcl-canvas); }
  .head[data-selected], .milestone[data-selected] { background: var(--arcl-surface-subtle); box-shadow: inset 2px 0 0 var(--arcl-focus); }
  /* A head too narrow for its title wraps the badge and buttons below it. */
  .toggle { display: flex; flex: 1 1 14rem; align-items: baseline; gap: var(--arcl-space-2); min-width: 0; padding: var(--arcl-space-2) 0; border: 0; color: var(--arcl-text); background: transparent; font: inherit; text-align: start; cursor: pointer; }
  .chevron { width: 1rem; color: var(--arcl-text-muted); }
  .title { overflow: hidden; color: var(--arcl-heading); font: 600 var(--arcl-type-sm) / 1.4 var(--arcl-font-ui); text-overflow: ellipsis; }
  .spec-title .title { font-size: var(--arcl-type-base); }
  .detail { color: var(--arcl-text-muted); font-size: var(--arcl-type-xs); white-space: nowrap; }
  .empty { padding: var(--arcl-space-3) var(--arcl-space-6); border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); }
  .milestones { margin: 0; padding: 0; list-style: none; }
  .milestone { display: grid; grid-template-columns: 1.25rem minmax(0, 1fr) minmax(7rem, 9rem) minmax(6rem, 9rem); align-items: center; gap: var(--arcl-space-3); width: 100%; min-height: 3rem; padding: var(--arcl-space-2) var(--arcl-space-4) var(--arcl-space-2) var(--arcl-space-8); border: 0; border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); color: var(--arcl-text); background: transparent; font: inherit; text-align: start; cursor: pointer; }
  .milestone:hover { background: var(--arcl-surface-hover); }
  .ring { color: var(--arcl-info); }
  .milestone[data-done] .ring { color: var(--arcl-success); }
  .milestone[data-done] .title { color: var(--arcl-text-muted); font-weight: 500; }
  .progress { display: grid; gap: var(--arcl-space-1); }
  .bar { display: block; height: 4px; overflow: hidden; border-radius: 2px; background: var(--arcl-surface-subtle); }
  .bar span { display: block; height: 4px; background: var(--arcl-success); }
  .now { font-size: var(--arcl-type-xs); }
  .ready { color: var(--arcl-info); }
  .blocked { color: var(--arcl-danger); }
  .tree { padding: var(--arcl-space-2) var(--arcl-space-4) var(--arcl-space-3) var(--arcl-space-8); border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); background: var(--arcl-canvas); }
  .loose { display: grid; gap: var(--arcl-space-2); }
  .planning-form { display: grid; gap: var(--arcl-space-3); max-width: 48rem; margin: var(--arcl-space-3) 0 var(--arcl-space-4); padding: var(--arcl-space-4); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); }
  .spec .planning-form { margin: var(--arcl-space-3) var(--arcl-space-4); }
  .planning-form h2 { margin: 0; color: var(--arcl-heading); font: 600 var(--arcl-type-base) / 1.3 var(--arcl-font-ui); }
  .planning-form :global(textarea[spellcheck="false"]) { min-height: 16rem; font: var(--arcl-type-sm) / 1.5 var(--arcl-font-code); }
  .buttons { display: flex; gap: var(--arcl-space-2); }
  @media (max-width: 48rem) {
    .milestone { grid-template-columns: 1.25rem minmax(0, 1fr); padding-inline-start: var(--arcl-space-4); }
    .progress, .now { grid-column: 2; }
    .head.plan, .tree, .empty { padding-inline-start: var(--arcl-space-4); }
  }
</style>
