<script module lang="ts">
  /** Props for the heading over a group of task rows. */
  export interface TaskGroupHeaderProps {
    title: string;
    /** Text after the title, such as a count. */
    detail?: string;
    /** 2 for a top-level group, 3 for a section inside one. */
    level?: 2 | 3;
    id?: string;
  }
</script>

<script lang="ts">
  let { title, detail, level = 2, id }: TaskGroupHeaderProps = $props();
</script>

<svelte:element this={`h${level}`} class="arcl-task-group-header" data-level={level} {id}>
  <span>{title}</span>
  {#if detail}<span class="arcl-task-group-header__detail">{detail}</span>{/if}
</svelte:element>

<style>
  .arcl-task-group-header { display: flex; align-items: center; gap: var(--arcl-space-3); min-height: 2.5rem; margin: 0; padding: 0 var(--arcl-space-4); border-bottom: var(--arcl-border-width) solid var(--arcl-border); color: var(--arcl-heading); background: var(--arcl-surface-subtle); font: 600 var(--arcl-type-sm) / 1.3 var(--arcl-font-ui); }
  [data-level="3"] { min-height: 2rem; border-bottom-color: var(--arcl-surface-subtle); color: var(--arcl-text-muted); background: var(--arcl-surface); font-size: var(--arcl-type-xs); }
  .arcl-task-group-header__detail { color: var(--arcl-text-muted); font-weight: 400; }
</style>
