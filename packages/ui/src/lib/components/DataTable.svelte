<script module lang="ts">
  import type { Snippet } from "svelte";

  /** One column of a data table. */
  export interface DataTableColumn {
    /** Names the column to `cell`, and keys it, so each column needs its own. */
    id: string;
    /** The header text. */
    label: string;
    /** A CSS width, such as `7rem`. Columns without one share the rest. */
    width?: string;
    /** Hide the column while the table is narrower than 32rem. */
    hideNarrow?: boolean;
  }

  /**
   * Props for a table whose rows the keyboard selects one at a time. Each
   * row's first cell holds the button that selects it, and Tab reaches one
   * row: the selected one, or the first. From a row, `ArrowDown` and
   * `ArrowUp` select the next and previous rows and `Home` and `End` the
   * first and last, through `onselect`, and prevent the key's default so a
   * page handler that checks `defaultPrevented` does not move again. The
   * table scrolls inside its own box, under a sticky header, so give it a
   * height or a `max-height`.
   */
  export interface DataTableProps<Row> {
    rows: Row[];
    columns: DataTableColumn[];
    /** The row's ID, which `selectedId` and `onselect` use. */
    rowId: (row: Row) => string;
    /** The content of one cell. */
    cell: Snippet<[Row, DataTableColumn]>;
    /** The ID of the selected row, or `null` for none. */
    selectedId?: string | null;
    /** Called with a row's ID when the user clicks the row or presses its button. */
    onselect?: (id: string) => void;
    /** The accessible name of the table's region. */
    label: string;
  }
</script>

<script lang="ts" generics="Row">
  let { rows, columns, rowId, cell, selectedId = null, onselect, label }: DataTableProps<Row> = $props();
  let root: HTMLDivElement;
  let ids = $derived(rows.map(rowId));
  let tabbableId = $derived(selectedId !== null && ids.includes(selectedId) ? selectedId : ids[0]);

  /** Select another row than `from` with the arrow keys, `Home` or `End`. */
  function onkeydown(event: KeyboardEvent, from: string) {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    const index = ids.indexOf(from);
    const targets: Record<string, number> = { ArrowDown: index + 1, ArrowUp: index - 1, Home: 0, End: ids.length - 1 };
    const next = targets[event.key];
    if (next === undefined) return;
    event.preventDefault();
    const id = ids[Math.min(ids.length - 1, Math.max(0, next))];
    if (id !== from) onselect?.(id);
  }

  // Keep focus on the selected row while focus is in the table or nowhere, so
  // moving the selection moves focus without taking it from a form.
  $effect(() => {
    if (selectedId === null) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && !root.contains(active)) return;
    const button = root.querySelector<HTMLButtonElement>(`[data-row-id="${CSS.escape(selectedId)}"]`);
    if (button && button !== active) button.focus();
  });
</script>

<div class="arcl-data-table" bind:this={root} role="region" aria-label={label}>
  <table>
    <thead>
      <tr>
        {#each columns as column (column.id)}
          <th scope="col" style:width={column.width} data-hide-narrow={column.hideNarrow || undefined}
            >{column.label}</th
          >
        {/each}
      </tr>
    </thead>
    <tbody>
      {#each rows as row (rowId(row))}
        {@const id = rowId(row)}
        <tr data-selected={id === selectedId || undefined} onclick={() => onselect?.(id)}>
          {#each columns as column, index (column.id)}
            <td data-hide-narrow={column.hideNarrow || undefined}>
              {#if index === 0}
                <button
                  type="button"
                  data-row-id={id}
                  aria-current={id === selectedId ? "true" : undefined}
                  tabindex={id === tabbableId ? 0 : -1}
                  onkeydown={(event) => onkeydown(event, id)}
                >
                  {@render cell(row, column)}
                </button>
              {:else}
                {@render cell(row, column)}
              {/if}
            </td>
          {/each}
        </tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .arcl-data-table {
    container-type: inline-size;
    min-width: 0;
    overflow: auto;
    font-size: var(--arcl-type-sm);
  }
  table {
    width: 100%;
    border-collapse: separate;
    border-spacing: 0;
    table-layout: fixed;
  }
  th {
    position: sticky;
    z-index: 1;
    top: 0;
    padding: var(--arcl-space-2) var(--arcl-space-3);
    border-bottom: var(--arcl-border-width) solid var(--arcl-border);
    color: var(--arcl-text-muted);
    background: var(--arcl-surface);
    font: 600 var(--arcl-type-xs) / 1.4 var(--arcl-font-ui);
    text-align: start;
    white-space: nowrap;
  }
  td {
    height: 2.5rem;
    padding: 0 var(--arcl-space-3);
    overflow: hidden;
    border-bottom: var(--arcl-border-width) solid var(--arcl-surface-subtle);
    color: var(--arcl-text-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  td:first-child {
    padding: 0;
  }
  tr {
    cursor: pointer;
  }
  tbody tr:hover {
    background: var(--arcl-surface-hover);
  }
  tr[data-selected] {
    background: var(--arcl-surface-subtle);
  }
  tr[data-selected] td:first-child {
    box-shadow: inset 2px 0 0 var(--arcl-focus);
  }
  button {
    display: block;
    width: 100%;
    height: 2.5rem;
    padding: 0 var(--arcl-space-3);
    overflow: hidden;
    border: 0;
    color: var(--arcl-text);
    background: transparent;
    font: 500 var(--arcl-type-sm) / 2.5rem var(--arcl-font-ui);
    text-align: start;
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: pointer;
  }
  /* The focus ring goes round the whole row, inside the table's scroll box. */
  button:focus-visible {
    outline: none;
  }
  tr:has(button:focus-visible) {
    outline: var(--arcl-focus-width) solid var(--arcl-focus);
    outline-offset: calc(-1 * var(--arcl-focus-width));
  }
  @container (max-width: 32rem) {
    [data-hide-narrow] {
      display: none;
    }
  }
</style>
