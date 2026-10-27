<script module lang="ts">
  import type { TaskGroup } from "../types";
  /** Props for a dense task list in groups and sections. */
  export interface TaskListProps {
    groups: TaskGroup[];
    /** The link each row opens. */
    href: (id: string) => string;
    selectedId?: string | null;
    onselect?: (id: string) => void;
    label?: string;
  }

  /** The row IDs of `groups` in the order the list shows them. */
  export function rowOrder(groups: TaskGroup[]): string[] {
    return groups.flatMap((group) => group.sections.flatMap((section) => section.rows.map((row) => row.id)));
  }
</script>

<script lang="ts">
  import TaskGroupHeader from "./TaskGroupHeader.svelte";
  import TaskRow from "./TaskRow.svelte";

  let { groups, href, selectedId = null, onselect, label = "Tasks" }: TaskListProps = $props();
  const baseId = $props.id();
  let root: HTMLDivElement;
  let order = $derived(rowOrder(groups));
  // One row is reachable with Tab: the selected one, or the first.
  let tabbableId = $derived(selectedId !== null && order.includes(selectedId) ? selectedId : order[0]);

  // Keep focus on the selected row while focus is in the list or nowhere, so
  // moving the selection moves focus without taking it from a filter.
  $effect(() => {
    if (selectedId === null) return;
    const active = document.activeElement;
    if (active !== null && active !== document.body && !root.contains(active)) return;
    const link = root.querySelector<HTMLAnchorElement>(`[data-task-id="${CSS.escape(selectedId)}"] a`);
    if (link && link !== active) link.focus();
  });
</script>

<div class="arcl-task-list" bind:this={root} role="region" aria-label={label}>
  {#each groups as group, groupIndex (group.id)}
    <section aria-labelledby={`${baseId}-${groupIndex}`}>
      <TaskGroupHeader id={`${baseId}-${groupIndex}`} title={group.title} detail={group.detail} />
      {#each group.sections as section (section.id)}
        <TaskGroupHeader level={3} title={section.title} detail={String(section.rows.length)} />
        <ul aria-label={`${group.title}: ${section.title}`}>
          {#each section.rows as row (row.id)}
            <TaskRow
              {row}
              href={href(row.id)}
              selected={row.id === selectedId}
              tabbable={row.id === tabbableId}
              {onselect} />
          {/each}
        </ul>
      {/each}
    </section>
  {/each}
</div>

<style>
  .arcl-task-list {
    overflow: visible;
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-md);
    background: var(--arcl-surface);
  }
  section + section {
    border-top: var(--arcl-border-width) solid var(--arcl-border);
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
</style>
