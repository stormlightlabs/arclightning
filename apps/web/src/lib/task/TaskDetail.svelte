<script module lang="ts">
  import type { ChangeWarning, TaskDetail } from "$lib/data/types";
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
    /**
     * Set by the view: whether an open form holds input that leaving would
     * lose, such as a typed handoff note or a changed field.
     */
    unsaved?: boolean;
    /**
     * The warnings the last saved change gave, shown until dismissed. The
     * view sets them after each change; a caller may pass a change's
     * warnings in, such as those of the create that opened the view.
     */
    warnings?: ChangeWarning[];
  }
</script>

<script lang="ts">
  import { tick, untrack } from "svelte";
  import {
    Button,
    CopyId,
    Feedback,
    Field,
    Markdown,
    MarkdownEditor,
    PropertiesPanel,
    SelectMenu,
    StatusBadge,
  } from "@arclightning/ui";
  import { useCommands } from "$lib/commands/registry.svelte";
  import type { Command } from "$lib/commands/menu";
  import { QUEUED, type Task } from "$lib/data/types";
  import { dataSource } from "$lib/data/source";
  import { failureNotice } from "$lib/notice";
  import { editWaitsFor } from "$lib/sync/edits";
  import { syncStatus } from "$lib/sync/status.svelte";
  import { PRIORITIES } from "$lib/work/filters";
  import { taskHref } from "$lib/work/links";
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
  import {
    blockersOf,
    copyLabel,
    propertiesOf,
    reasonSegments,
    showsReadiness,
    titlesOf,
    type DoneBlocker,
  } from "./view";

  let {
    detail,
    workHref,
    panelOpen = $bindable(true),
    onchanged,
    initialAction = null,
    unsaved = $bindable(false),
    warnings = $bindable([]),
  }: TaskDetailProps = $props();
  let showSource = $state(false);

  /** The action running now, `"edit"` while an edit saves, or `"file"` while the raw file loads or saves. */
  let busy = $state<TaskAction | "edit" | "file" | null>(null);
  /** The action whose Markdown form is open. */
  let asking = $state<TaskAction | null>(null);
  let text = $state("");
  /** The open form's editor, which takes focus when the form opens. */
  let askEditor = $state<ReturnType<typeof MarkdownEditor>>();
  /** The last failed action and how it failed, shown next to that action. */
  let failure = $state<{ action: TaskAction | "edit" | "file"; failure: ActionFailure } | null>(null);
  /** The tasks the last completion unblocked, by ID. */
  let unblocked = $state<string[] | null>(null);
  let draft = $state<TaskDraft | null>(null);
  /** Why the task did not reload after a saved change. */
  let stale = $state<ActionFailure | null>(null);
  /**
   * The raw file editor: the file as loaded, its revision and the edited
   * text, or `null` while it is closed. Only a source that can save the file
   * or queue its save offers it.
   */
  let file = $state<{ path: string; source: string; revision: string; text: string } | null>(null);
  const source = dataSource();
  const canEditFile = source.recordSource !== undefined && source.saveRecordSource !== undefined;

  const priorities = PRIORITIES.map((priority) => ({
    value: priority,
    label: priority[0].toUpperCase() + priority.slice(1),
  }));

  let task = $derived(detail.task);
  let titles = $derived(titlesOf(detail));
  let blockers = $derived(blockersOf(detail));
  let reasons = $derived(detail.readiness.reasons);
  let reasonCount = $derived(reasons.length === 1 ? "1 reason" : `${reasons.length} reasons`);
  let actions = $derived(actionsFor(task.status));
  let edit = $derived(draft === null ? {} : editOf(task, draft));
  /**
   * Whether an edit to this task waits for the next sync. Applying it
   * rewrites the file, so a raw edit from the file as last synced would fail
   * with the conflict error; the view offers none until then.
   */
  let fileQueued = $derived(editWaitsFor(syncStatus(), task.id));

  $effect(() => {
    unsaved =
      (asking !== null && text.trim() !== "") ||
      Object.keys(edit).length > 0 ||
      (file !== null && file.text !== file.source);
  });

  // The menu and the shortcuts run the same `choose` as the buttons.
  useCommands(() => ({
    context: { id: task.id, label: task.title },
    commands:
      busy !== null
        ? []
        : [
            ...actions.map((action): Command => ({
              id: `task:${action}`,
              label: ACTION_LABELS[action],
              scope: "selection",
              keys: [ACTION_KEYS[action]],
              icon: action === "complete" ? "check" : "task",
              run: () => choose(action),
            })),
            ...(draft === null
              ? [
                  {
                    id: "task:edit",
                    label: "Edit",
                    scope: "selection",
                    keys: [EDIT_KEY],
                    icon: "edit",
                    run: openEdit,
                  } satisfies Command,
                ]
              : []),
            ...(canEditFile && file === null && !fileQueued
              ? [
                  {
                    id: "task:edit-file",
                    label: "Edit file",
                    scope: "selection",
                    icon: "editFile",
                    run: openFile,
                  } satisfies Command,
                ]
              : []),
          ],
  }));

  // Only open the form: an initial action never runs a change by itself.
  $effect(() => {
    if (initialAction === null || ACTION_TEXT[initialAction] === undefined) return;
    if (!untrack(() => actions).includes(initialAction)) return;
    const action = initialAction;
    untrack(() => void ask(action));
  });

  function openEdit() {
    draft = draftOf(task);
    file = null;
    failure = null;
    unblocked = null;
  }

  /** Load the task's workspace file into the raw editor, replacing any edit in it. */
  async function openFile() {
    if (!source.recordSource || busy !== null || fileQueued) return;
    draft = null;
    failure = null;
    unblocked = null;
    busy = "file";
    try {
      const loaded = await source.recordSource(task.id);
      file = { path: loaded.path, source: loaded.source, revision: loaded.revision, text: loaded.source };
    } catch (error) {
      failure = { action: "file", failure: failureOf(error) };
    } finally {
      busy = null;
    }
  }

  /** Save the raw file. `arcl` checks it as it checks a workspace file it imports. */
  function saveFile(event: SubmitEvent) {
    event.preventDefault();
    const save = source.saveRecordSource;
    if (file === null || !save) return;
    const { text: edited, revision } = file;
    failure = null;
    unblocked = null;
    void perform(
      "file",
      () => save(task.id, { source: edited, revision }),
      () => (file = null),
    );
  }

  /**
   * Run `action`, or open its Markdown form when it takes text. Choosing an
   * action whose form is open, by button, shortcut or menu, only refocuses
   * the form's editor: the form's own submit button sends the change.
   */
  function choose(action: TaskAction) {
    if (ACTION_TEXT[action] !== undefined) {
      void ask(action);
      return;
    }
    failure = null;
    unblocked = null;
    void perform(
      action,
      () => runAction(dataSource(), task.id, action),
      () => (asking = null),
    );
  }

  /** Open `action`'s Markdown form, or keep the one open, and focus its editor. */
  async function ask(action: TaskAction) {
    if (asking !== action) {
      asking = action;
      text = "";
      failure = null;
      unblocked = null;
    }
    await tick();
    askEditor?.focus();
  }

  /** Send the open form's action with its text. */
  function submitAsk(event: SubmitEvent) {
    event.preventDefault();
    const action = asking;
    if (action === null || busy !== null) return;
    failure = null;
    unblocked = null;
    void perform(
      action,
      () => runAction(dataSource(), task.id, action, text),
      () => (asking = null),
      (result) => {
        if (action === "complete") unblocked = result.unblocked;
      },
    );
  }

  function saveEdit(event: SubmitEvent) {
    event.preventDefault();
    failure = null;
    unblocked = null;
    void perform(
      "edit",
      () => dataSource().updateTask(task.id, edit),
      () => (draft = null),
    );
  }

  /**
   * Run `change`, reload the task, then `close` its form and call `done`.
   * A failed change stays next to `action`, and its form stays open; a
   * failed reload after a saved change says so. A queued change also closes
   * its form, with the queued notice, so it is not queued twice.
   */
  async function perform<T extends { warnings: ChangeWarning[] }>(
    action: TaskAction | "edit" | "file",
    change: () => Promise<T>,
    close: () => void,
    done?: (result: T) => void,
  ) {
    busy = action;
    stale = null;
    warnings = [];
    try {
      const result = await change();
      warnings = result.warnings;
      await onchanged?.().catch((error: unknown) => (stale = failureOf(error)));
      close();
      done?.(result);
    } catch (error) {
      failure = { action, failure: failureOf(error) };
      if (failure.failure.code === QUEUED) close();
    } finally {
      busy = null;
    }
  }
</script>

{#snippet failed(action: TaskAction | "edit" | "file", what: string)}
  {#if failure?.action === action}
    <Feedback {...failureNotice(failure.failure, `Could not ${what}`)} />
  {/if}
{/snippet}

{#snippet relation(related: Task, where: string)}
  <li>
    <a href={taskHref(related.id)}>{related.title}</a>
    <CopyId id={related.id} label={copyLabel(related, where)} />
    <StatusBadge status={related.status} />
  </li>
{/snippet}

{#snippet done({ blocker, ancestor }: DoneBlocker)}
  <li>
    <div class="row">
      <a href={taskHref(blocker.task.id)}>{blocker.task.title}</a>
      <CopyId
        id={blocker.task.id}
        label={copyLabel(
          blocker.task,
          ancestor ? `Completed blockers inherited from ${ancestor.title} (${ancestor.id})` : "Completed blockers",
        )} />
      <StatusBadge status={blocker.task.status} />
    </div>
    {#if ancestor}<p class="muted via">Inherited from <a href={taskHref(ancestor.id)}>{ancestor.title}</a></p>{/if}
    {#if blocker.evidence}
      <Markdown source={blocker.evidence} label={`Evidence for ${blocker.task.title}`} />
    {:else}
      <p class="muted">No evidence recorded.</p>
    {/if}
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
      <div class="id"><CopyId id={task.id} /></div>
      <h1 id="task-title">{task.title}</h1>
      <div class="actions" role="group" aria-label="Task actions">
        {#each actions as action, index (action)}
          <Button
            label={ACTION_LABELS[action]}
            variant={index === 0 ? "primary" : "secondary"}
            busy={busy === action}
            disabled={busy !== null}
            aria-expanded={ACTION_TEXT[action] === undefined ? undefined : asking === action}
            onclick={() => choose(action)} />
        {/each}
        {#if draft === null}
          <Button label="Edit" icon="edit" variant="ghost" disabled={busy !== null} onclick={openEdit} />
        {/if}
        {#if canEditFile && file === null}
          <Button
            label="Edit file"
            icon="editFile"
            variant="ghost"
            busy={busy === "file"}
            disabled={busy !== null || fileQueued}
            onclick={openFile} />
        {/if}
      </div>
      {#if canEditFile && fileQueued}
        <p class="muted">An edit to this task waits for the next sync. Edit the file after that sync.</p>
      {/if}
      {#if asking !== null}
        {@const label = ACTION_TEXT[asking] ?? ""}
        <form class="ask" aria-label={ACTION_LABELS[asking]} onsubmit={submitAsk}>
          <MarkdownEditor
            bind:this={askEditor}
            value={text}
            label={`${label} Markdown`}
            readonly={false}
            onchange={(value) => (text = value)} />
          <div class="buttons">
            <Button
              type="submit"
              variant="primary"
              label={ACTION_LABELS[asking]}
              busy={busy === asking}
              disabled={busy !== null} />
            <Button
              label="Cancel"
              variant="ghost"
              disabled={busy !== null}
              onclick={() => ((asking = null), (failure = null))} />
          </div>
        </form>
      {/if}
      {#each warnings as warning, index (index)}
        <Feedback
          tone="warning"
          title="The change is saved, with a warning"
          message={`${warning.message} (${warning.code})`}>
          {#snippet actions()}
            <Button
              label="Dismiss"
              variant="ghost"
              onclick={() => (warnings = warnings.filter((_, other) => other !== index))} />
          {/snippet}
        </Feedback>
      {/each}
      {#if stale !== null}
        <Feedback
          tone="warning"
          title="The change is saved, but this view did not reload"
          message={`${stale.message} (${stale.code})`} />
      {/if}
      {#each actions as action (action)}{@render failed(
          action,
          `${ACTION_LABELS[action].toLowerCase()} this task`,
        )}{/each}
      {#if file === null}{@render failed("file", "open the file")}{/if}
      {#if draft === null}{@render failed("edit", "save the task")}{/if}
      {#if unblocked !== null}
        <div class="notice" role="status" aria-label="Completed">
          <strong>Completed.</strong>
          {#if unblocked.length}
            Unblocked:
            <ul>
              {#each unblockedTasks(unblocked, detail) as ready (ready.id)}
                <li>
                  <a href={taskHref(ready.id)}>{ready.title}</a>
                  <CopyId id={ready.id} label={copyLabel(ready, "Unblocked")} />
                </li>
              {/each}
            </ul>
          {:else}
            No task was waiting on this one.
          {/if}
        </div>
      {/if}
    </header>

    {#if showsReadiness(detail)}
      <section class="readiness" data-ready={detail.readiness.ready} aria-label="Readiness">
        <p>
          <strong>{detail.readiness.ready ? "Ready to start" : "Not ready"}</strong>
          {#if reasons.length}<span class="muted">· {reasonCount}</span>{/if}
        </p>
        {#if reasons.length}
          <ul>
            {#each reasons as reason}
              <li data-kind={reason.kind}>
                {#each reasonSegments(reason, titles) as segment, part (part)}
                  {#if "href" in segment}<a href={segment.href} title={segment.id}>{segment.label}</a
                    >{:else}{segment.text}{/if}
                {/each}
                <code class="kind">{reason.kind}</code>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    {#if file !== null}
      <form class="edit" aria-labelledby="task-file" onsubmit={saveFile}>
        <div class="section-head">
          <h2 id="task-file">Edit file</h2>
          <code class="path">{file.path}</code>
        </div>
        <p class="muted">
          TOML front matter between the <code>+++</code> lines, then the Markdown body. <code>arcl</code> checks the file
          as it checks a workspace file it imports.
        </p>
        <MarkdownEditor
          value={file.text}
          label="Record file"
          frontMatter
          readonly={false}
          onchange={(value) => file && (file.text = value)} />
        <div class="buttons">
          <Button
            type="submit"
            variant="primary"
            label="Save file"
            busy={busy === "file"}
            disabled={busy !== null || fileQueued || file.text === file.source} />
          <Button
            label="Cancel"
            variant="ghost"
            disabled={busy !== null}
            onclick={() => ((file = null), (failure = null))} />
        </div>
        {#if failure?.action === "file"}
          <Feedback {...failureNotice(failure.failure, "Could not save the file")}>
            {#snippet actions()}
              {#if failure?.failure.code === "conflict"}<Button label="Reload the file" onclick={openFile} />{/if}
            {/snippet}
          </Feedback>
        {/if}
      </form>
    {:else if draft !== null}
      <form class="edit" aria-labelledby="task-edit" onsubmit={saveEdit}>
        <h2 id="task-edit">Edit task</h2>
        <Field label="Title" bind:value={draft.title} required />
        <SelectMenu
          label="Priority"
          options={priorities}
          value={draft.priority}
          onchange={(value) => draft && (draft.priority = value as TaskDraft["priority"])} />
        <span class="label">Description</span>
        <MarkdownEditor
          value={draft.body}
          label="Description Markdown"
          readonly={false}
          onchange={(value) => draft && (draft.body = value)} />
        <div class="buttons">
          <Button
            type="submit"
            variant="primary"
            label="Save"
            busy={busy === "edit"}
            disabled={busy !== null || Object.keys(edit).length === 0} />
          <Button
            label="Cancel"
            variant="ghost"
            disabled={busy !== null}
            onclick={() => ((draft = null), (failure = null))} />
        </div>
        {@render failed("edit", "save the task")}
      </form>
    {:else}
      <section aria-labelledby="task-description">
        <div class="section-head">
          <h2 id="task-description">Description</h2>
          {#if task.body}
            <button
              type="button"
              class="link-button"
              aria-pressed={showSource}
              onclick={() => (showSource = !showSource)}>
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

    {#if blockers.direct.length || blockers.inherited.length}
      <section aria-labelledby="task-blockers">
        <h2 id="task-blockers">Blocked by</h2>
        {#if blockers.direct.length}
          <ul class="relations" aria-label="Direct blockers">
            {#each blockers.direct as blocker (blocker.task.id)}{@render relation(blocker.task, "Blocked by")}{/each}
          </ul>
        {/if}
        {#each blockers.inherited as group (group.ancestor.id)}
          <h3>Inherited from <a href={taskHref(group.ancestor.id)}>{group.ancestor.title}</a></h3>
          <ul class="relations" aria-label={`Inherited from ${group.ancestor.title}`}>
            {#each group.blockers as blocker (blocker.task.id)}{@render relation(
                blocker.task,
                `Inherited from ${group.ancestor.title} (${group.ancestor.id})`,
              )}{/each}
          </ul>
        {/each}
      </section>
    {/if}

    {#if blockers.done.length}
      <section aria-labelledby="task-done-blockers">
        <h2 id="task-done-blockers">Completed blockers</h2>
        <ul class="relations done">
          {#each blockers.done as entry (`${entry.ancestor?.id ?? ""}/${entry.blocker.task.id}`)}{@render done(
              entry,
            )}{/each}
        </ul>
      </section>
    {/if}

    {#if detail.children.length}
      <section aria-labelledby="task-children">
        <h2 id="task-children">Subtasks</h2>
        <ul class="relations">
          {#each detail.children as child (child.id)}{@render relation(child, "Subtasks")}{/each}
        </ul>
      </section>
    {/if}

    <section aria-labelledby="task-unblocks">
      <h2 id="task-unblocks">Unblocks</h2>
      {#if detail.dependents.length}
        <ul class="relations">
          {#each detail.dependents as dependent (dependent.id)}{@render relation(dependent, "Unblocks")}{/each}
        </ul>
      {:else}
        <p class="muted">No other task waits on this one.</p>
      {/if}
    </section>

    <section aria-labelledby="task-activity">
      <h2 id="task-activity">Activity</h2>
      <div class="entry" aria-labelledby="task-handoff">
        <h3 id="task-handoff">Latest handoff</h3>
        {#if task.handoff}<Markdown source={task.handoff} label="Latest handoff" />{:else}<p class="muted">
            No handoff recorded.
          </p>{/if}
      </div>
      <div class="entry" aria-labelledby="task-evidence">
        <h3 id="task-evidence">Evidence</h3>
        {#if task.evidence}<Markdown source={task.evidence} label="Evidence" />{:else}<p class="muted">
            No evidence recorded.
          </p>{/if}
      </div>
    </section>
  </article>

  <div class="panel">
    <PropertiesPanel properties={propertiesOf(detail)} bind:open={panelOpen} shortcut="]" />
  </div>
</div>

<style>
  .breadcrumb ol {
    display: flex;
    flex-wrap: wrap;
    gap: var(--arcl-space-2);
    margin: 0 0 var(--arcl-space-4);
    padding: 0;
    color: var(--arcl-text-muted);
    font-size: var(--arcl-type-sm);
    list-style: none;
  }
  .breadcrumb li + li::before {
    content: "/";
    margin-inline-end: var(--arcl-space-2);
    color: var(--arcl-border-strong);
  }
  .breadcrumb a {
    color: inherit;
    text-decoration: none;
  }
  .breadcrumb a:hover {
    color: var(--arcl-text);
    text-decoration: underline;
  }
  .breadcrumb [aria-current] {
    color: var(--arcl-text);
  }
  .layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 18rem;
    align-items: start;
    gap: var(--arcl-space-6);
  }
  .layout[data-panel="closed"] {
    grid-template-columns: minmax(0, 1fr) 10rem;
  }
  .panel {
    position: sticky;
    top: var(--arcl-space-4);
  }
  article {
    display: grid;
    gap: var(--arcl-space-5);
    min-width: 0;
  }
  header {
    display: grid;
    gap: var(--arcl-space-1);
  }
  .id {
    display: flex;
    font-size: var(--arcl-type-sm);
  }
  h1 {
    margin: 0;
    color: var(--arcl-heading);
    font: 650 var(--arcl-type-xl) / 1.25 var(--arcl-font-ui);
    text-wrap: balance;
  }
  h2 {
    margin: 0 0 var(--arcl-space-2);
    color: var(--arcl-heading);
    font: 600 var(--arcl-type-base) / 1.3 var(--arcl-font-ui);
  }
  h3 {
    margin: var(--arcl-space-3) 0 var(--arcl-space-2);
    color: var(--arcl-text-muted);
    font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui);
  }
  h3 a,
  .relations a,
  .readiness a {
    color: var(--arcl-link);
    text-decoration: none;
  }
  h3 a:hover,
  .relations a:hover,
  .readiness a:hover {
    text-decoration: underline;
  }
  .muted {
    margin: 0;
    color: var(--arcl-text-muted);
  }
  .readiness {
    padding: var(--arcl-space-3) var(--arcl-space-4);
    border: var(--arcl-border-width) solid var(--arcl-warning);
    border-radius: var(--arcl-radius-sm);
    background: var(--arcl-warning-surface);
  }
  .readiness[data-ready="true"] {
    border-color: var(--arcl-success);
    background: var(--arcl-success-surface);
  }
  .readiness p {
    margin: 0;
  }
  .readiness strong {
    color: var(--arcl-heading);
    font-family: var(--arcl-font-ui);
  }
  .readiness ul {
    margin: var(--arcl-space-2) 0 0;
    padding-inline-start: 1.25rem;
  }
  .readiness li {
    color: var(--arcl-text);
  }
  .kind {
    margin-inline-start: var(--arcl-space-2);
    color: var(--arcl-text-muted);
    font: var(--arcl-type-xs) var(--arcl-font-code);
  }
  .path {
    overflow: hidden;
    color: var(--arcl-text-muted);
    font: var(--arcl-type-xs) var(--arcl-font-code);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .edit .muted code {
    font: var(--arcl-type-xs) var(--arcl-font-code);
  }
  .section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--arcl-space-3);
  }
  .link-button {
    padding: 0;
    border: 0;
    color: var(--arcl-link);
    background: none;
    font: inherit;
    font-size: var(--arcl-type-sm);
    cursor: pointer;
  }
  .link-button:hover {
    text-decoration: underline;
  }
  .relations {
    margin: 0;
    padding: 0;
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-md);
    background: var(--arcl-surface);
    list-style: none;
  }
  .relations li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--arcl-space-3);
    min-height: 2.25rem;
    padding: 0 var(--arcl-space-3);
    font-size: var(--arcl-type-sm);
  }
  .relations li + li {
    border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle);
  }
  .relations a {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .relations :global(.arcl-copy-id) {
    font-size: var(--arcl-type-xs);
  }
  .done li {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--arcl-space-1);
    padding: var(--arcl-space-2) var(--arcl-space-3);
  }
  .done .row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    align-items: center;
    gap: var(--arcl-space-3);
  }
  .done .via {
    font-size: var(--arcl-type-xs);
  }
  .done .via a {
    color: var(--arcl-link);
    text-decoration: none;
  }
  .entry + .entry {
    margin-top: var(--arcl-space-2);
  }
  .actions,
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--arcl-space-2);
    margin-top: var(--arcl-space-2);
  }
  .ask,
  .edit {
    display: grid;
    gap: var(--arcl-space-3);
    margin-top: var(--arcl-space-2);
  }
  .edit .buttons {
    margin-top: 0;
  }
  .label {
    color: var(--arcl-text-muted);
    font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui);
  }
  .notice {
    margin-top: var(--arcl-space-2);
    padding: var(--arcl-space-3) var(--arcl-space-4);
    border-inline-start: 4px solid var(--arcl-success);
    border-radius: var(--arcl-radius-sm);
    background: var(--arcl-success-surface);
    font-size: var(--arcl-type-sm);
  }
  .notice ul {
    margin: var(--arcl-space-1) 0 0;
    padding-inline-start: 1.25rem;
  }
  .notice a {
    color: var(--arcl-link);
  }
  .notice :global(.arcl-copy-id) {
    font-size: var(--arcl-type-xs);
  }
  @media (max-width: 60rem) {
    .layout,
    .layout[data-panel="closed"] {
      grid-template-columns: minmax(0, 1fr);
    }
    .panel {
      position: static;
      grid-row: 1;
    }
  }
</style>
