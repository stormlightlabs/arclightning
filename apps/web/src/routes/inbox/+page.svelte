<script lang="ts">
  import { Button, Dialog, EmptyState, Feedback, Markdown, type IconName, type SelectOption } from "@arclightning/ui";
  import { onMount, tick, untrack } from "svelte";
  import { formatKeys, useCommands, type Command } from "$lib/commands";
  import {
    dataSource,
    latestRead,
    type Capture,
    type CaptureSummary,
    type ChangeWarning,
    type Promoted,
    type WorkList,
  } from "$lib/data";
  import { TRIAGE_KEYS, TRIAGE_LABELS, afterRemoval, formFor, promotionOf, type PromotionForm, type TriageAction } from "$lib/inbox";
  import PromoteForm from "$lib/inbox/PromoteForm.svelte";
  import { failureOf, type ActionFailure } from "$lib/task";
  import { moveSelection, orderMilestones, orderTasks, taskHref, visibleSelection, workKeyAction } from "$lib/work";

  const ACTIONS: TriageAction[] = ["task", "spec", "note", "discard"];
  const ICONS: Record<TriageAction, IconName> = { task: "task", spec: "spec", note: "note", discard: "close" };

  let captures = $state<CaptureSummary[] | null>(null);
  let failure = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let order = $derived(captures?.map((capture) => capture.id) ?? []);
  // The first capture is selected until another is.
  let selected = $derived(visibleSelection(order, selectedId) ?? order[0] ?? null);
  let selectedCapture = $derived(captures?.find((capture) => capture.id === selected) ?? null);

  /** The selected capture with its body, once read. */
  let detail = $state<Capture | null>(null);
  let detailFailure = $state<string | null>(null);
  /** The open promotion form, or `null`. */
  let form = $state<PromotionForm | null>(null);
  /** Milestones and tasks for the task form, read when it first opens. */
  let work = $state<WorkList | null>(null);
  let busy = $state(false);
  let actionFailure = $state<ActionFailure | null>(null);
  /** What the last triage did, with a link to the new record where it has a view. */
  let notice = $state<{ text: string; href?: string; label?: string } | null>(null);
  let warnings = $state<ChangeWarning[]>([]);
  let confirming = $state(false);
  let list = $state<HTMLElement>();
  let pane = $state<HTMLElement>();

  let phases = $derived<SelectOption[]>(
    work ? orderMilestones(work).filter((phase) => phase.status === "open").map((phase) => ({ value: phase.id, label: phase.title })) : [],
  );
  let parents = $derived<SelectOption[]>(
    work
      ? orderTasks(work.items, orderMilestones(work))
          .filter(({ task }) => task.status !== "completed" && task.status !== "cancelled")
          .map(({ task }) => ({ value: task.id, label: task.title }))
      : [],
  );

  const readList = latestRead((signal) => dataSource().captures(signal), {
    loaded: (loaded) => (captures = loaded),
    failed: (error) => (failure = error instanceof Error ? error.message : String(error)),
  });
  // The capture the read in flight asks for.
  let requested = "";
  const readDetail = latestRead((signal) => dataSource().capture(requested, signal), {
    loaded: (loaded) => (detail = loaded),
    failed: (error) => (detailFailure = error instanceof Error ? error.message : String(error)),
  });

  onMount(() => {
    load();
    return () => {
      readList.stop();
      readDetail.stop();
    };
  });

  // A new selection closes the form and reads the capture's body.
  $effect(() => {
    const id = selected;
    untrack(() => {
      form = null;
      actionFailure = null;
      showDetail(id);
    });
  });

  // Keep focus on the selected row while focus is in the list or nowhere.
  $effect(() => {
    const id = selected;
    if (id === null || !list) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && !list.contains(active)) return;
    list.querySelector<HTMLElement>(`[data-capture-id="${CSS.escape(id)}"]`)?.focus();
  });

  useCommands(() => ({
    context: selectedCapture && { id: selectedCapture.id, label: selectedCapture.title },
    commands: [
      ...(selectedCapture === null || busy
        ? []
        : ACTIONS.map(
            (action): Command => ({
              id: `inbox:${action}`,
              label: TRIAGE_LABELS[action],
              scope: "selection",
              keys: [TRIAGE_KEYS[action]],
              icon: ICONS[action],
              run: () => triage(action),
            }),
          )),
      { id: "inbox:next", label: "Select the next capture", scope: "view", group: "Inbox", keys: ["j"], viewKeys: true, run: () => move(1) },
      { id: "inbox:previous", label: "Select the previous capture", scope: "view", group: "Inbox", keys: ["k"], viewKeys: true, run: () => move(-1) },
    ],
  }));

  function load() {
    failure = null;
    captures = null;
    readList.start();
  }

  function showDetail(id: string | null) {
    detail = null;
    detailFailure = null;
    if (id === null) {
      readDetail.stop();
      return;
    }
    requested = id;
    readDetail.start();
  }

  function move(step: 1 | -1) {
    selectedId = moveSelection(order, selected, step);
  }

  function triage(action: TriageAction) {
    if (action === "discard") confirming = true;
    else void openForm(action);
  }

  /** Open the form that promotes the selected capture, and focus its title. */
  async function openForm(to: Exclude<TriageAction, "discard">) {
    const capture = selectedCapture;
    if (capture === null) return;
    notice = null;
    actionFailure = null;
    if (to === "task" && work === null) {
      try {
        work = await dataSource().work();
      } catch (error) {
        actionFailure = failureOf(error);
        return;
      }
      if (selected !== capture.id) return;
    }
    const tasks = work?.items.map(({ task }) => ({ id: task.id, title: task.title, status: task.status })) ?? [];
    form = formFor(to, detail?.title ?? capture.title, tasks);
    await tick();
    pane?.querySelector<HTMLElement>(".promote input")?.focus();
  }

  async function promote() {
    const capture = selectedCapture;
    if (form === null || capture === null || busy) return;
    busy = true;
    actionFailure = null;
    try {
      const { value, warnings: given } = await dataSource().promoteCapture(capture.id, promotionOf(form));
      warnings = given;
      notice = promotedNotice(capture, value);
      // A new task changes the tasks the form lists as parents.
      work = null;
      remove(capture.id);
    } catch (error) {
      actionFailure = failureOf(error);
    } finally {
      busy = false;
    }
  }

  async function discard() {
    const capture = selectedCapture;
    confirming = false;
    if (capture === null || busy) return;
    busy = true;
    actionFailure = null;
    notice = null;
    try {
      warnings = (await dataSource().discardCapture(capture.id)).warnings;
      notice = { text: `Discarded “${capture.title}”.` };
      remove(capture.id);
    } catch (error) {
      actionFailure = failureOf(error);
    } finally {
      busy = false;
    }
  }

  /** Drop `id` from the list and select the capture after it. */
  function remove(id: string) {
    selectedId = afterRemoval(order, id);
    captures = captures?.filter((capture) => capture.id !== id) ?? null;
  }

  function promotedNotice(capture: CaptureSummary, made: Promoted) {
    switch (made.kind) {
      case "task":
        return { text: `Promoted “${capture.title}” to the task “${made.title}”.`, href: taskHref(made.id), label: "Open task" };
      case "spec":
        return { text: `Promoted “${capture.title}” to the spec “${made.title}”.`, href: "/planning", label: "Open Planning" };
      case "note":
        return { text: `Made the note “${made.title}” (${made.id}) from “${capture.title}”.` };
    }
  }

  function created(capture: Capture): string {
    const date = new Date(capture.created_at);
    return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  }

  function onkeydown(event: KeyboardEvent) {
    // Escape closes the form, unless an open listbox or the dialog took it.
    if (event.key === "Escape" && form !== null && !confirming && !event.defaultPrevented) {
      form = null;
      event.preventDefault();
      return;
    }
    const action = workKeyAction(event);
    if (action === null || action === "open") return;
    move(action === "next" ? 1 : -1);
    event.preventDefault();
  }
</script>

<svelte:head><title>Inbox · Arc Lightning</title></svelte:head>
<svelte:window {onkeydown} />

<header>
  <h1>Inbox</h1>
  {#if captures !== null}<span class="count">{captures.length === 1 ? "1 open capture" : `${captures.length} open captures`}</span>{/if}
</header>

{#if notice !== null}
  <p class="notice" role="status">
    {notice.text}
    {#if notice.href}<a href={notice.href}>{notice.label}</a>{/if}
  </p>
{/if}
{#each warnings as warning, index (index)}
  <Feedback tone="warning" title="The change is saved, with a warning" message={`${warning.message} (${warning.code})`}>
    {#snippet actions()}
      <Button label="Dismiss" variant="ghost" onclick={() => (warnings = warnings.filter((_, other) => other !== index))} />
    {/snippet}
  </Feedback>
{/each}

{#if failure !== null}
  <Feedback tone="danger" title="Could not load the Inbox" message={failure}>
    {#snippet actions()}
      <Button label="Retry" onclick={load} />
    {/snippet}
  </Feedback>
{:else if captures === null}
  <p class="loading" role="status">Loading captures…</p>
{:else if captures.length === 0}
  <EmptyState title="The Inbox is empty" message="Press Q to capture an idea. Captures wait here until you promote or discard them." />
{:else}
  <div class="columns">
    <section class="list" aria-label="Captures" bind:this={list}>
      <ul>
        {#each captures as capture (capture.id)}
          <li>
            <button
              type="button"
              data-capture-id={capture.id}
              aria-current={capture.id === selected ? "true" : undefined}
              tabindex={capture.id === selected ? 0 : -1}
              onclick={() => (selectedId = capture.id)}
            >
              {capture.title}
            </button>
          </li>
        {/each}
      </ul>
    </section>

    <section class="triage" aria-label="Triage" bind:this={pane}>
      {#if selectedCapture !== null}
        <p class="meta">Capture{#if detail !== null && created(detail)}{` · ${created(detail)}`}{/if}</p>
        <h2>{detail?.title ?? selectedCapture.title}</h2>
        {#if detailFailure !== null}
          <Feedback tone="danger" title="Could not load this capture" message={detailFailure}>
            {#snippet actions()}
              <Button label="Retry" onclick={() => showDetail(selected)} />
            {/snippet}
          </Feedback>
        {:else if detail === null}
          <p class="loading" role="status">Loading capture…</p>
        {:else if detail.body !== ""}
          <Markdown source={detail.body} label="Capture body" />
        {/if}
        <div class="actions" role="group" aria-label="Triage actions">
          {#each ACTIONS as action (action)}
            <Button
              variant={action === "task" ? "primary" : action === "discard" ? "ghost" : "secondary"}
              aria-keyshortcuts={TRIAGE_KEYS[action]}
              disabled={busy}
              onclick={() => triage(action)}
            >
              <kbd aria-hidden="true">{formatKeys([TRIAGE_KEYS[action]], false)[0]}</kbd>
              {TRIAGE_LABELS[action]}
            </Button>
          {/each}
        </div>
        {#if form !== null}
          <PromoteForm bind:form {phases} {parents} {busy} onsubmit={promote} oncancel={() => (form = null)} />
        {/if}
        {#if actionFailure !== null}
          <Feedback tone="danger" title="Could not triage this capture" message={`${actionFailure.message} (${actionFailure.code})`} />
        {/if}
      {/if}
    </section>
  </div>
{/if}

<Dialog bind:open={confirming} title="Discard this capture?" description="A discarded capture leaves the Inbox for good.">
  <div class="confirm">
    <p>{selectedCapture?.title}</p>
    <div class="buttons">
      <!-- svelte-ignore a11y_autofocus -->
      <Button variant="danger" label="Discard" autofocus onclick={discard} />
      <Button variant="ghost" label="Cancel" onclick={() => (confirming = false)} />
    </div>
  </div>
</Dialog>

<style>
  header { display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--arcl-space-3); margin-bottom: var(--arcl-space-4); }
  h1 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.2 var(--arcl-font-ui); }
  .count, .meta, .loading { margin: 0; color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); }
  .notice { display: flex; flex-wrap: wrap; gap: var(--arcl-space-2); margin: 0 0 var(--arcl-space-4); padding: var(--arcl-space-2) var(--arcl-space-3); border-inline-start: 4px solid var(--arcl-success); border-radius: var(--arcl-radius-sm); background: var(--arcl-success-surface); font-size: var(--arcl-type-sm); }
  .notice a { color: var(--arcl-link); }
  .columns { display: flex; flex-wrap: wrap; align-items: stretch; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); }
  /* The list scrolls on its own, so the triage pane stays in view. */
  .list { flex: 1 1 18rem; max-width: 28rem; max-height: calc(100dvh - 10rem); overflow-y: auto; border-inline-end: var(--arcl-border-width) solid var(--arcl-border); }
  ul { margin: 0; padding: 0; list-style: none; }
  li + li { border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); }
  li button { display: block; width: 100%; padding: var(--arcl-space-3) var(--arcl-space-4); border: 0; color: var(--arcl-text); background: transparent; font: 500 var(--arcl-type-sm) / 1.4 var(--arcl-font-ui); text-align: start; cursor: pointer; }
  li button:hover { background: var(--arcl-surface-hover); }
  li button[aria-current="true"] { background: var(--arcl-surface-subtle); box-shadow: inset 2px 0 0 var(--arcl-focus); }
  .triage { display: grid; flex: 999 1 24rem; align-content: start; gap: var(--arcl-space-4); min-width: 0; padding: var(--arcl-space-5) var(--arcl-space-6); }
  h2 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-lg) / 1.3 var(--arcl-font-ui); }
  .actions { display: flex; flex-wrap: wrap; gap: var(--arcl-space-2); }
  kbd { display: inline-flex; min-width: 1.125rem; height: 1.125rem; align-items: center; justify-content: center; margin-inline-end: var(--arcl-space-1); padding: 0 0.25rem; border: var(--arcl-border-width) solid currentColor; border-radius: 4px; font: 500 var(--arcl-type-xs) / 1 var(--arcl-font-ui); opacity: 0.8; }
  .confirm { display: grid; gap: var(--arcl-space-4); }
  .confirm p { margin: 0; font-weight: 600; }
  .buttons { display: flex; gap: var(--arcl-space-2); }
  @media (max-width: 40rem) { .list { max-width: none; max-height: 40dvh; border-inline-end: 0; border-bottom: var(--arcl-border-width) solid var(--arcl-border); } .triage { padding: var(--arcl-space-4); } }
</style>
