<script module lang="ts">
  import type { Snippet } from "svelte";

  /** A labelled panel in the shared tab set. */
  export interface TabItem { id: string; label: string; panel: Snippet; disabled?: boolean; }
  /** Props for accessible tabs with arrow-key navigation. */
  export interface TabsProps { label: string; items: TabItem[]; selected?: string; onselect?: (id: string) => void; }
</script>

<script lang="ts">
  let { label, items, selected = $bindable(), onselect }: TabsProps = $props();
  let tablist: HTMLDivElement;
  let active = $derived(selected ?? items.find((item) => !item.disabled)?.id);

  function select(id: string) { selected = id; onselect?.(id); }
  function move(event: KeyboardEvent) {
    if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
    const tabs = [...tablist.querySelectorAll<HTMLButtonElement>("[role=tab]:not(:disabled)")];
    const current = tabs.indexOf(document.activeElement as HTMLButtonElement);
    const index = event.key === "Home" ? 0 : event.key === "End" ? tabs.length - 1 : (current + (event.key === "ArrowRight" ? 1 : -1) + tabs.length) % tabs.length;
    event.preventDefault();
    tabs[index]?.focus();
    tabs[index]?.click();
  }
</script>

<div class="arcl-tabs">
  <div class="arcl-tabs__list" bind:this={tablist} role="tablist" aria-label={label}>
    {#each items as item (item.id)}
      <button
        type="button"
        role="tab"
        id={`tab-${item.id}`}
        aria-controls={`panel-${item.id}`}
        aria-selected={active === item.id}
        tabindex={active === item.id ? 0 : -1}
        disabled={item.disabled}
        onkeydown={move}
        onclick={() => select(item.id)}
      >{item.label}</button>
    {/each}
  </div>
  {#each items as item (item.id)}
    {#if active === item.id}
      <div class="arcl-tabs__panel" role="tabpanel" id={`panel-${item.id}`} aria-labelledby={`tab-${item.id}`}>
        {@render item.panel()}
      </div>
    {/if}
  {/each}
</div>

<style>
  .arcl-tabs__list { display: flex; gap: var(--arcl-space-1); overflow-x: auto; border-bottom: 1px solid var(--arcl-border); }
  [role="tab"] { min-height: var(--arcl-control-height); padding-inline: var(--arcl-space-4); border: 0; border-bottom: 3px solid transparent; color: var(--arcl-text-muted); background: transparent; font-family: var(--arcl-font-ui); font-weight: 650; cursor: pointer; }
  [role="tab"]:hover { color: var(--arcl-text); background: var(--arcl-surface-hover); }
  [role="tab"][aria-selected="true"] { color: var(--arcl-heading); border-bottom-color: var(--arcl-accent); }
  [role="tab"]:disabled { opacity: 0.52; cursor: not-allowed; }
  .arcl-tabs__panel { padding-block: var(--arcl-space-4); }
</style>
