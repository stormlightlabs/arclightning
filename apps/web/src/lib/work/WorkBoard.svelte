<script module lang="ts">
  import type { TaskStatus } from "$lib/data/types";
  import type { TaskAction } from "$lib/task/actions";
  import type { BoardCard, BoardColumn } from "./board";

  /** Props for the Work page's board of status columns. */
  export interface WorkBoardProps {
    columns: BoardColumn[];
    selectedId?: string | null;
    /** The card whose move is running, shown busy. */
    busyId?: string | null;
    /**
     * Whether a task change is running. While one is, no card moves: drags,
     * drops, Move buttons and `move` do nothing, so a second change cannot
     * start before the first one's reply.
     */
    locked?: boolean;
    onselect?: (id: string) => void;
    /**
     * Run `action` on `card`, with the completion evidence in `text`. The
     * board only asks for a move a task command allows.
     */
    onmove: (card: BoardCard, action: TaskAction, text: string) => Promise<void>;
    /** A card dropped on a column no task command moves it to. The card stays where it is. */
    onrefuse: (card: BoardCard, to: TaskStatus) => void;
  }
</script>

<script lang="ts">
  import {
    Button,
    CopyId,
    Dialog,
    MarkdownEditor,
    PriorityBadge,
    TaskGlyph,
    formatGlyph,
    formatStatus,
  } from "@arclightning/ui";
  import { tick } from "svelte";
  import { moveAction, moveTargets } from "./board";
  import { taskHref } from "./links";

  let {
    columns,
    selectedId = null,
    busyId = null,
    locked = false,
    onselect,
    onmove,
    onrefuse,
  }: WorkBoardProps = $props();
  const baseId = $props.id();
  let root: HTMLDivElement;

  /** The card being dragged, and the column it is over. */
  let dragging = $state<BoardCard | null>(null);
  let over = $state<TaskStatus | null>(null);

  /** The card whose Move dialog is open, and whether it asks for completion evidence. */
  let moving = $state<{ card: BoardCard; completing: boolean } | null>(null);
  let dialogOpen = $state(false);
  let evidence = $state("");
  let evidenceEditor = $state<ReturnType<typeof MarkdownEditor>>();

  // Keep the selected card in view, and focus it while focus is on the board
  // or nowhere, as the list does with its rows.
  $effect(() => {
    if (selectedId === null) return;
    const link = root.querySelector<HTMLAnchorElement>(`[data-task-id="${CSS.escape(selectedId)}"] a`);
    if (!link) return;
    link.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    const active = document.activeElement;
    if (active === null || active === document.body || (root.contains(active) && active.tagName === "A")) {
      if (link !== active) link.focus();
    }
  });

  /** Move `card` to the `to` column: run the command, ask for evidence first, or refuse. */
  function moveTo(card: BoardCard, to: TaskStatus) {
    if (locked || card.status === to) return;
    const action = moveAction(card.status, to);
    if (action === null) {
      onrefuse(card, to);
      return;
    }
    if (action === "complete") {
      void openMove(card, true);
      return;
    }
    dialogOpen = false;
    void onmove(card, action, "");
  }

  async function openMove(card: BoardCard, completing: boolean) {
    if (locked) return;
    moving = { card, completing };
    evidence = "";
    dialogOpen = true;
    if (completing) {
      await tick();
      evidenceEditor?.focus();
    }
  }

  function complete(event: SubmitEvent) {
    event.preventDefault();
    if (moving === null || locked) return;
    const { card } = moving;
    dialogOpen = false;
    void onmove(card, "complete", evidence);
  }

  function ondragstart(event: DragEvent, card: BoardCard) {
    if (locked) {
      event.preventDefault();
      return;
    }
    dragging = card;
    event.dataTransfer?.setData("text/plain", card.id);
    if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
  }

  function ondragover(event: DragEvent, status: TaskStatus) {
    if (dragging === null || locked) return;
    // Every column takes the drop, so a move no command makes is refused
    // with a message rather than ignored.
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = "move";
    over = status;
  }

  function ondrop(event: DragEvent, status: TaskStatus) {
    event.preventDefault();
    const card = dragging;
    dragging = null;
    over = null;
    if (card !== null) moveTo(card, status);
  }

  /** Open the Move dialog of the card `id`, such as the selected one, when the board shows it. */
  export function move(id: string) {
    const card = columns.flatMap((column) => column.cards).find((candidate) => candidate.id === id);
    if (card) void openMove(card, false);
  }
</script>

<div class="board" bind:this={root} role="region" aria-label="Board" aria-describedby={`${baseId}-help`}>
  <p class="hidden" id={`${baseId}-help`}>Drag a card to another column, or press M or its Move button to move it.</p>
  {#each columns as column (column.status)}
    <section
      class="column"
      data-status={column.status}
      data-over={over === column.status || undefined}
      aria-labelledby={`${baseId}-${column.status}`}
      ondragover={(event) => ondragover(event, column.status)}
      ondragleave={() => over === column.status && (over = null)}
      ondrop={(event) => ondrop(event, column.status)}>
      <h2 id={`${baseId}-${column.status}`}>{column.title} <span class="count">{column.cards.length}</span></h2>
      <ul aria-labelledby={`${baseId}-${column.status}`}>
        {#each column.cards as card (card.id)}
          <li
            class="card"
            data-task-id={card.id}
            data-selected={card.id === selectedId || undefined}
            aria-busy={card.id === busyId}
            draggable={!locked}
            ondragstart={(event) => ondragstart(event, card)}
            ondragend={() => ((dragging = null), (over = null))}>
            <div class="top">
              <span class="id"><CopyId id={card.id} label={`Copy the ID of ${card.title} (${card.id})`} /></span>
              <PriorityBadge priority={card.priority} />
            </div>
            <a
              href={taskHref(card.id)}
              aria-current={card.id === selectedId ? "true" : undefined}
              onfocus={() => onselect?.(card.id)}>{card.title}</a>
            <div class="bottom">
              <span class="readiness" title={card.reasons.join("\n") || undefined}>
                <TaskGlyph kind={card.glyph} />
                {formatGlyph(card.glyph)}
              </span>
              <Button
                label="Move"
                variant="ghost"
                class="move"
                aria-label={`Move ${card.title}`}
                aria-haspopup="dialog"
                busy={card.id === busyId}
                disabled={locked}
                onclick={() => openMove(card, false)} />
            </div>
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>

<Dialog
  bind:open={dialogOpen}
  title={moving === null
    ? "Move task"
    : moving.completing
      ? `Complete ${moving.card.title}`
      : `Move ${moving.card.title}`}
  description={moving === null ? undefined : `In ${formatStatus(moving.card.status)}`}>
  {#if moving !== null && moving.completing}
    <form class="complete" aria-label="Complete" onsubmit={complete}>
      <MarkdownEditor
        bind:this={evidenceEditor}
        value={evidence}
        label="Evidence Markdown"
        readonly={false}
        onchange={(value) => (evidence = value)} />
      <div class="buttons">
        <Button type="submit" variant="primary" label="Complete" />
        <Button label="Cancel" variant="ghost" onclick={() => (dialogOpen = false)} />
      </div>
    </form>
  {:else if moving !== null}
    {@const card = moving.card}
    {@const targets = moveTargets(card.status)}
    {#if targets.length === 0}
      <p class="muted">No task command moves a {formatStatus(card.status).toLowerCase()} task.</p>
    {:else}
      <ul class="targets" aria-label="Move to">
        {#each targets as target (target)}
          <li>
            <Button
              label={formatStatus(target)}
              variant={target === targets[0] ? "primary" : "secondary"}
              onclick={() => moveTo(card, target)} />
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</Dialog>

<style>
  .board {
    display: grid;
    grid-auto-columns: minmax(11.5rem, 1fr);
    grid-auto-flow: column;
    gap: var(--arcl-space-2);
    overflow-x: auto;
    padding-bottom: var(--arcl-space-2);
  }
  .column {
    display: flex;
    min-width: 0;
    max-height: calc(100dvh - 13rem);
    min-height: 12rem;
    flex-direction: column;
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-md);
    background: var(--arcl-surface);
  }
  .column[data-over] {
    outline: var(--arcl-focus-width) dashed var(--arcl-focus);
    outline-offset: -2px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: var(--arcl-space-2);
    margin: 0;
    padding: var(--arcl-space-3) var(--arcl-space-3) var(--arcl-space-2);
    color: var(--arcl-heading);
    font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui);
  }
  .count {
    color: var(--arcl-text-muted);
    font-weight: 500;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .column ul {
    display: grid;
    flex: 1;
    /* Size cards to the column, not to the full ID's width. */
    grid-template-columns: minmax(0, 1fr);
    align-content: start;
    gap: var(--arcl-space-2);
    overflow-y: auto;
    padding: 0 var(--arcl-space-2) var(--arcl-space-2);
  }
  .card {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--arcl-space-1);
    padding: var(--arcl-space-2) var(--arcl-space-3);
    border: var(--arcl-border-width) solid var(--arcl-surface-hover);
    border-radius: var(--arcl-radius-sm);
    background: var(--arcl-surface-subtle);
    font-size: var(--arcl-type-sm);
    cursor: grab;
  }
  .card:hover {
    border-color: var(--arcl-border);
  }
  .card[data-selected] {
    border-color: var(--arcl-link);
    box-shadow: inset 2px 0 0 var(--arcl-link);
  }
  .card[aria-busy="true"] {
    opacity: 0.6;
    cursor: progress;
  }
  .top,
  .bottom {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--arcl-space-2);
  }
  .id {
    display: flex;
    min-width: 0;
    font-size: var(--arcl-type-xs);
  }
  .card a {
    color: var(--arcl-text);
    font-weight: 550;
    text-decoration: none;
    overflow-wrap: anywhere;
  }
  .card a:hover {
    text-decoration: underline;
  }
  .readiness {
    display: inline-flex;
    align-items: center;
    gap: var(--arcl-space-1);
    color: var(--arcl-text-muted);
    font-size: var(--arcl-type-xs);
  }
  .bottom :global(.move) {
    min-height: 1.75rem;
    padding-inline: var(--arcl-space-2);
    font-size: var(--arcl-type-xs);
  }
  .targets {
    display: flex;
    flex-wrap: wrap;
    gap: var(--arcl-space-2);
  }
  .complete {
    display: grid;
    gap: var(--arcl-space-3);
  }
  .buttons {
    display: flex;
    gap: var(--arcl-space-2);
  }
  .muted {
    margin: 0;
    color: var(--arcl-text-muted);
  }
  .hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
</style>
