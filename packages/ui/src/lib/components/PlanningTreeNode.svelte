<script lang="ts">
  import type { PlanningTreeNode as Node } from "../types";
  import PlanningTreeNode from "./PlanningTreeNode.svelte";
  import StatusBadge from "./StatusBadge.svelte";

  let { node, selectedId, onselect }: { node: Node; selectedId?: string; onselect?: (node: Node) => void } = $props();
  let expanded = $state(true);
  let hasChildren = $derived(Boolean(node.children?.length));
</script>

<li>
  <div class="arcl-tree-node" data-selected={selectedId === node.id || undefined}>
    {#if hasChildren}
      <button class="arcl-tree-node__toggle" type="button" aria-label={`${expanded ? "Collapse" : "Expand"} ${node.title}`} aria-expanded={expanded} onclick={() => expanded = !expanded}>
        <span aria-hidden="true">{expanded ? "−" : "+"}</span>
      </button>
    {:else}<span class="arcl-tree-node__spacer"></span>{/if}
    <button class="arcl-tree-node__record" type="button" onclick={() => onselect?.(node)}>
      <span>{node.title}</span><small>{node.kind}</small>
    </button>
    {#if node.status}<StatusBadge status={node.status} />{/if}
  </div>
  {#if hasChildren && expanded}
    <ul>
      {#each node.children ?? [] as child (child.id)}
        <PlanningTreeNode node={child} {selectedId} {onselect} />
      {/each}
    </ul>
  {/if}
</li>

<style>
  li { list-style: none; }
  ul { margin: 0 0 0 1.15rem; padding: 0; border-inline-start: 1px solid var(--arcl-border); }
  .arcl-tree-node { display: grid; grid-template-columns: 2.5rem minmax(0, 1fr) auto; align-items: center; gap: var(--arcl-space-1); min-height: 3.25rem; padding-inline: var(--arcl-space-2); border-radius: var(--arcl-radius-md); }
  .arcl-tree-node:hover, .arcl-tree-node[data-selected] { background: var(--arcl-surface-hover); }
  button { color: var(--arcl-text); }
  .arcl-tree-node__toggle { width: 2.5rem; height: 2.5rem; border: 0; border-radius: var(--arcl-radius-sm); background: transparent; font-size: 1.25rem; cursor: pointer; }
  .arcl-tree-node__record { display: grid; min-width: 0; padding: var(--arcl-space-2); border: 0; background: transparent; text-align: left; cursor: pointer; }
  .arcl-tree-node__record span { overflow: hidden; color: var(--arcl-heading); font-family: var(--arcl-font-ui); font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .arcl-tree-node__record small { color: var(--arcl-text-muted); text-transform: capitalize; }
  .arcl-tree-node__spacer { width: 2.5rem; }
</style>
