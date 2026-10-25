<script module lang="ts">
  import type { TaskDetail } from "$lib/data";
  import type { TaskAction } from "./actions";

  /** Props for the body of the Task view, once its task has loaded. */
  export interface TaskDetailProps {
    detail: TaskDetail;
    /** Where the breadcrumb's Work link goes. */
    workHref: string;
    /** Whether the properties panel is expanded. */
    panelOpen?: boolean;
    /** Reload `detail` after a change; the view waits for it before it shows the result. */
    onchanged?: () => Promise<void>;
    /**
     * An action whose form opens once the view shows, such as one the Work
     * list's menu asked for. An action without a form is ignored.
     */
    initialAction?: TaskAction | null;
  }
</script>

<script lang="ts">
  import { untrack } from "svelte";
  import { Button, Feedback, Field, Markdown, MarkdownEditor, PropertiesPanel, SelectMenu, StatusBadge } from "@arclightning/ui";
  import { useCommands, type Command } from "$lib/commands";
  import { dataSource, type Task } from "$lib/data";
  import { PRIORITIES, taskHref } from "$lib/work";
  import {
    ACTION_KEYS,
    ACTION_LABELS,
    ACTION_TEXT,
    EDIT_KEY,
    actionsFor,
    draftOf,
    editOf,
    failureOf,
    runAction,
    unblockedTasks,
    type ActionFailure,
    type TaskDraft,
  } from "./actions";
  import { inheritedGroups, propertiesOf, reasonSegments, titlesOf } from "./view";

  let { detail, workHref, panelOpen = $bindable(true), onchanged, initialAction = null }: TaskDetailProps = $props();
  let showSource = $state(false);

  /** The action running now, or `"edit"` while an edit saves. */
  let busy = $state<TaskAction | "edit" | null>(null);
  /** The action whose Markdown form is open. */
  let asking = $state<TaskAction | null>(null);
  let text = $state("");
  /** The last failed action and how it failed, shown next to that action. */
  let failure = $state<{ action: TaskAction | "edit"; failure: ActionFailure } | null>(null);
  /** The tasks the last completion unblocked, by ID. */
  let unblocked = $state<string[] | null>(null);
  let draft = $state<TaskDraft | null>(null);
  /** Why the task did not reload after a saved change. */
  let stale = $state<ActionFailure | null>(null);

  const priorities = PRIORITIES.map((priority) => ({ value: priority, label: priority[0].toUpperCase() + priority.slice(1) }));

  let task = $derived(detail.task);
  let titles = $derived(titlesOf(detail));
  let inherited = $derived(inheritedGroups(detail));
  let reasons = $derived(detail.readiness.reasons);
  let reasonCount = $derived(reasons.length === 1 ? "1 reason" : `${reasons.length} reasons`);
  let actions = $derived(actionsFor(task.status));
  let edit = $derived(draft === null ? {} : editOf(task, draft));

  // The menu and the shortcuts run the same `choose` as the buttons.
  useCommands(() => ({
    context: { id: task.id, label: task.title },
    commands: busy !== null ? [] : [
      ...actions.map((action): Command => ({
        id: `task:${action}`,
        label: ACTION_LABELS[action],
        scope: "selection",
        keys: [ACTION_KEYS[action]],
        icon: action === "complete" ? "check" : "task",
        run: () => choose(action),
      })),
      ...(draft === null ? [{ id: "task:edit", label: "Edit", scope: "selection", keys: [EDIT_KEY], icon: "markdown", run: openEdit } satisfies Command] : []),
    ],
  }));

  // Only open the form: an initial action never runs a change by itself.
  $effect(() => {
    if (initialAction === null || ACTION_TEXT[initialAction] === undefined) return;
    if (!untrack(() => actions).includes(initialAction)) return;
    untrack(() => {
      asking = initialAction;
      text = "";
      failure = null;
    });
  });

  function openEdit() {
    draft = draftOf(task);
    failure = null;
    unblocked = null;
  }

  /** Run `action`, or open its Markdown form first when it takes text. */
  function choose(action: TaskAction) {
    failure = null;
    unblocked = null;
    if (ACTION_TEXT[action] !== undefined && asking !== action) {
      asking = action;
      text = "";
      return;
    }
    void perform(action, () => runAction(dataSource(), task.id, action, text), (result) => {
      asking = null;
      if (action === "complete") unblocked = result.unblocked;
    });
  }

  function saveEdit(event: SubmitEvent) {
    event.preventDefault();
    failure = null;
    unblocked = null;
    void perform("edit", () => dataSource().updateTask(task.id, edit), () => (draft = null));
  }

  /**
   * Run `change`, reload the task, then call `done`. A failed change stays
   * next to `action`; a failed reload after a saved change says so.
   */
  async function perform<T>(action: TaskAction | "edit", change: () => Promise<T>, done: (result: T) => void) {
    busy = action;
    stale = null;
    try {
      const result = await change();
      await onchanged?.().catch((error: unknown) => (stale = failureOf(error)));
      done(result);
    } catch (error) {
      failure = { action, failure: failureOf(error) };
    } finally {
      busy = null;
    }
  }
</script>

{#snippet failed(action: TaskAction | "edit", what: string)}
  {#if failure?.action === action}
    <Feedback tone="danger" title={`Could not ${what}`} message={`${failure.failure.message} (${failure.failure.code})`} />
  {/if}
{/snippet}

{#snippet relation(related: Task)}
  <li>
    <a href={taskHref(related.id)}>{related.title}</a>
    <code>{related.id}</code>
    <StatusBadge status={related.status} />
  </li>
{/snippet}

<nav class="breadcrumb" aria-label="Breadcrumb">
  <ol>
    <li><a href={workHref}>Work</a></li>
    {#each detail.ancestors as ancestor (ancestor.id)}
      <li><a href={taskHref(ancestor.id)}>{ancestor.title}</a></li>
    {/each}
    <li aria-current="page">{task.title}</li>
  </ol>
</nav>

<div class="layout" data-panel={panelOpen ? "open" : "closed"}>
  <article aria-labelledby="task-title">
    <header>
      <code class="id">{task.id}</code>
      <h1 id="task-title">{task.title}</h1>
      <div class="actions" role="group" aria-label="Task actions">
        {#each actions as action, index (action)}
          <Button
            label={ACTION_LABELS[action]}
            variant={index === 0 ? "primary" : "secondary"}
            busy={busy === action}
            disabled={busy !== null}
            aria-expanded={ACTION_TEXT[action] === undefined ? undefined : asking === action}
            onclick={() => choose(action)}
          />
        {/each}
        {#if draft === null}
          <Button label="Edit" variant="ghost" disabled={busy !== null} onclick={openEdit} />
        {/if}
      </div>
      {#if asking !== null}
        {@const label = ACTION_TEXT[asking] ?? ""}
        <form class="ask" aria-label={ACTION_LABELS[asking]} onsubmit={(event) => (event.preventDefault(), choose(asking!))}>
          <MarkdownEditor value={text} label={`${label} Markdown`} readonly={false} onchange={(value) => (text = value)} />
          <div class="buttons">
            <Button type="submit" variant="primary" label={ACTION_LABELS[asking]} busy={busy === asking} disabled={busy !== null} />
            <Button label="Cancel" variant="ghost" disabled={busy !== null} onclick={() => ((asking = null), (failure = null))} />
          </div>
        </form>
      {/if}
      {#if stale !== null}
        <Feedback tone="warning" title="The change is saved, but this view did not reload" message={`${stale.message} (${stale.code})`} />
      {/if}
      {#each actions as action (action)}{@render failed(action, `${ACTION_LABELS[action].toLowerCase()} this task`)}{/each}
      {#if unblocked !== null}
        <div class="notice" role="status" aria-label="Completed">
          <strong>Completed.</strong>
          {#if unblocked.length}
            Unblocked:
            <ul>
              {#each unblockedTasks(unblocked, detail) as ready (ready.id)}
                <li><a href={taskHref(ready.id)}>{ready.title}</a> <code>{ready.id}</code></li>
              {/each}
            </ul>
          {:else}
            No task was waiting on this one.
          {/if}
        </div>
      {/if}
    </header>

    <section class="readiness" data-ready={detail.readiness.ready} aria-label="Readiness">
      <p>
        <strong>{detail.readiness.ready ? "Ready to start" : "Not ready"}</strong>
        {#if reasons.length}<span class="muted">· {reasonCount}</span>{/if}
      </p>
      {#if reasons.length}
        <ul>
          {#each reasons as reason, index (index)}
            <li data-kind={reason.kind}>
              {#each reasonSegments(reason, titles) as segment, part (part)}
                {#if "href" in segment}<a href={segment.href} title={segment.id}>{segment.label}</a>{:else}{segment.text}{/if}
              {/each}
              <code class="kind">{reason.kind}</code>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    {#if draft !== null}
      <form class="edit" aria-labelledby="task-edit" onsubmit={saveEdit}>
        <h2 id="task-edit">Edit task</h2>
        <Field label="Title" bind:value={draft.title} required />
        <SelectMenu label="Priority" options={priorities} value={draft.priority} onchange={(value) => draft && (draft.priority = value as TaskDraft["priority"])} />
        <span class="label">Description</span>
        <MarkdownEditor value={draft.body} label="Description Markdown" readonly={false} onchange={(value) => draft && (draft.body = value)} />
        <div class="buttons">
          <Button type="submit" variant="primary" label="Save" busy={busy === "edit"} disabled={busy !== null || Object.keys(edit).length === 0} />
          <Button label="Cancel" variant="ghost" disabled={busy !== null} onclick={() => ((draft = null), (failure = null))} />
        </div>
        {@render failed("edit", "save the task")}
      </form>
    {:else}
      <section aria-labelledby="task-description">
        <div class="section-head">
          <h2 id="task-description">Description</h2>
          {#if task.body}
            <button type="button" class="link-button" aria-pressed={showSource} onclick={() => (showSource = !showSource)}>
              {showSource ? "Show rendered" : "View Markdown"}
            </button>
          {/if}
        </div>
        {#if !task.body}
          <p class="muted">No description.</p>
        {:else if showSource}
          <MarkdownEditor value={task.body} label="Description Markdown" />
        {:else}
          <Markdown source={task.body} label="Description" />
        {/if}
      </section>
    {/if}

    {#if detail.blockers.length || inherited.length}
      <section aria-labelledby="task-blockers">
        <h2 id="task-blockers">Blocked by</h2>
        {#if detail.blockers.length}
          <ul class="relations" aria-label="Direct blockers">
            {#each detail.blockers as blocker (blocker.task.id)}{@render relation(blocker.task)}{/each}
          </ul>
        {/if}
        {#each inherited as group (group.ancestor.id)}
          <h3>Inherited from <a href={taskHref(group.ancestor.id)}>{group.ancestor.title}</a></h3>
          <ul class="relations" aria-label={`Inherited from ${group.ancestor.title}`}>
            {#each group.blockers as blocker (blocker.task.id)}{@render relation(blocker.task)}{/each}
          </ul>
        {/each}
      </section>
    {/if}

    {#if detail.children.length}
      <section aria-labelledby="task-children">
        <h2 id="task-children">Subtasks</h2>
        <ul class="relations">
          {#each detail.children as child (child.id)}{@render relation(child)}{/each}
        </ul>
      </section>
    {/if}

    <section aria-labelledby="task-unblocks">
      <h2 id="task-unblocks">Unblocks</h2>
      {#if detail.dependents.length}
        <ul class="relations">
          {#each detail.dependents as dependent (dependent.id)}{@render relation(dependent)}{/each}
        </ul>
      {:else}
        <p class="muted">No other task waits on this one.</p>
      {/if}
    </section>

    <section aria-labelledby="task-activity">
      <h2 id="task-activity">Activity</h2>
      <div class="entry" aria-labelledby="task-handoff">
        <h3 id="task-handoff">Latest handoff</h3>
        {#if task.handoff}<Markdown source={task.handoff} label="Latest handoff" />{:else}<p class="muted">No handoff recorded.</p>{/if}
      </div>
      <div class="entry" aria-labelledby="task-evidence">
        <h3 id="task-evidence">Evidence</h3>
        {#if task.evidence}<Markdown source={task.evidence} label="Evidence" />{:else}<p class="muted">No evidence recorded.</p>{/if}
      </div>
    </section>
  </article>

  <div class="panel">
    <PropertiesPanel properties={propertiesOf(detail)} bind:open={panelOpen} shortcut="]" />
  </div>
</div>

<style>
  .breadcrumb ol { display: flex; flex-wrap: wrap; gap: var(--arcl-space-2); margin: 0 0 var(--arcl-space-4); padding: 0; color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); list-style: none; }
  .breadcrumb li + li::before { content: "/"; margin-inline-end: var(--arcl-space-2); color: var(--arcl-border-strong); }
  .breadcrumb a { color: inherit; text-decoration: none; }
  .breadcrumb a:hover { color: var(--arcl-text); text-decoration: underline; }
  .breadcrumb [aria-current] { color: var(--arcl-text); }
  .layout { display: grid; grid-template-columns: minmax(0, 1fr) 18rem; align-items: start; gap: var(--arcl-space-6); }
  .layout[data-panel="closed"] { grid-template-columns: minmax(0, 1fr) 10rem; }
  .panel { position: sticky; top: var(--arcl-space-4); }
  article { display: grid; gap: var(--arcl-space-5); min-width: 0; }
  header { display: grid; gap: var(--arcl-space-1); }
  .id { color: var(--arcl-text-muted); font: var(--arcl-type-sm) var(--arcl-font-code); }
  h1 { margin: 0; color: var(--arcl-heading); font: 650 var(--arcl-type-xl) / 1.25 var(--arcl-font-ui); text-wrap: balance; }
  h2 { margin: 0 0 var(--arcl-space-2); color: var(--arcl-heading); font: 600 var(--arcl-type-base) / 1.3 var(--arcl-font-ui); }
  h3 { margin: var(--arcl-space-3) 0 var(--arcl-space-2); color: var(--arcl-text-muted); font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui); }
  h3 a, .relations a, .readiness a { color: var(--arcl-link); text-decoration: none; }
  h3 a:hover, .relations a:hover, .readiness a:hover { text-decoration: underline; }
  .muted { margin: 0; color: var(--arcl-text-muted); }
  .readiness { padding: var(--arcl-space-3) var(--arcl-space-4); border-inline-start: 4px solid var(--arcl-warning); border-radius: var(--arcl-radius-sm); background: var(--arcl-warning-surface); }
  .readiness[data-ready="true"] { border-color: var(--arcl-success); background: var(--arcl-success-surface); }
  .readiness p { margin: 0; }
  .readiness strong { color: var(--arcl-heading); font-family: var(--arcl-font-ui); }
  .readiness ul { margin: var(--arcl-space-2) 0 0; padding-inline-start: 1.25rem; }
  .readiness li { color: var(--arcl-text); }
  .kind { margin-inline-start: var(--arcl-space-2); color: var(--arcl-text-muted); font: var(--arcl-type-xs) var(--arcl-font-code); }
  .section-head { display: flex; align-items: baseline; justify-content: space-between; gap: var(--arcl-space-3); }
  .link-button { padding: 0; border: 0; color: var(--arcl-link); background: none; font: inherit; font-size: var(--arcl-type-sm); cursor: pointer; }
  .link-button:hover { text-decoration: underline; }
  .relations { margin: 0; padding: 0; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); list-style: none; }
  .relations li { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; align-items: center; gap: var(--arcl-space-3); min-height: 2.25rem; padding: 0 var(--arcl-space-3); font-size: var(--arcl-type-sm); }
  .relations li + li { border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); }
  .relations a { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .relations code { color: var(--arcl-text-muted); font: var(--arcl-type-xs) var(--arcl-font-code); }
  .entry + .entry { margin-top: var(--arcl-space-2); }
  .actions, .buttons { display: flex; flex-wrap: wrap; gap: var(--arcl-space-2); margin-top: var(--arcl-space-2); }
  .ask, .edit { display: grid; gap: var(--arcl-space-3); margin-top: var(--arcl-space-2); }
  .edit .buttons { margin-top: 0; }
  .label { color: var(--arcl-text-muted); font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui); }
  .notice { margin-top: var(--arcl-space-2); padding: var(--arcl-space-3) var(--arcl-space-4); border-inline-start: 4px solid var(--arcl-success); border-radius: var(--arcl-radius-sm); background: var(--arcl-success-surface); font-size: var(--arcl-type-sm); }
  .notice ul { margin: var(--arcl-space-1) 0 0; padding-inline-start: 1.25rem; }
  .notice a { color: var(--arcl-link); }
  .notice code { color: var(--arcl-text-muted); font: var(--arcl-type-xs) var(--arcl-font-code); }
  @media (max-width: 60rem) {
    .layout, .layout[data-panel="closed"] { grid-template-columns: minmax(0, 1fr); }
    .panel { position: static; grid-row: 1; }
  }
</style>
