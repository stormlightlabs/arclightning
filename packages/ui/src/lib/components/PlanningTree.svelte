<script module lang="ts">
  import type { PlanningTreeNode as PlanningNode } from "../types";
  /** Props for a collapsible application-supplied planning hierarchy. */
  export interface PlanningTreeProps { label?: string; nodes: PlanningNode[]; selectedId?: string; onselect?: (node: PlanningNode) => void; }
</script>

<script lang="ts">
  import PlanningTreeNode from "./PlanningTreeNode.svelte";
  let { label = "Planning records", nodes, selectedId, onselect }: PlanningTreeProps = $props();
</script>

<nav class="arcl-tree" aria-label={label}>
  <ul>
    {#each nodes as node (node.id)}
      <PlanningTreeNode {node} {selectedId} {onselect} />
    {/each}
  </ul>
</nav>

<style>
  .arcl-tree { color: var(--arcl-text); }
  ul { margin: 0; padding: 0; }
</style>
