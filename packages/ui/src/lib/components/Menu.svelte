<script module lang="ts">
  /** One action in the shared menu. */
  export interface MenuItem { id: string; label: string; disabled?: boolean; destructive?: boolean; }
  /** Props for a button-triggered action menu. */
  export interface MenuProps {
    items: MenuItem[];
    label?: string;
    onselect?: (item: MenuItem) => void;
  }
</script>

<script lang="ts">
  let { items, label = "More actions", onselect }: MenuProps = $props();
  let open = $state(false);
  let root: HTMLDivElement;

  function focusItem(index: number) {
    root.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)").item(index)?.focus();
  }

  function handleKeydown(event: KeyboardEvent) {
    const choices = [...root.querySelectorAll<HTMLButtonElement>("[role=menuitem]:not(:disabled)")];
    const current = choices.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") { event.preventDefault(); open = false; root.querySelector<HTMLButtonElement>("[aria-haspopup=menu]")?.focus(); }
    if (event.key === "ArrowDown") { event.preventDefault(); focusItem((current + 1) % choices.length); }
    if (event.key === "ArrowUp") { event.preventDefault(); focusItem((current - 1 + choices.length) % choices.length); }
    if (event.key === "Home") { event.preventDefault(); focusItem(0); }
    if (event.key === "End") { event.preventDefault(); focusItem(choices.length - 1); }
  }

  function choose(item: MenuItem) {
    if (item.disabled) return;
    open = false;
    onselect?.(item);
  }
</script>

<svelte:window onclick={(event) => { if (open && !root.contains(event.target as Node)) open = false; }} />

<div class="arcl-menu" bind:this={root}>
  <button
    class="arcl-menu__trigger"
    type="button"
    aria-haspopup="menu"
    aria-expanded={open}
    onclick={() => { open = !open; if (open) requestAnimationFrame(() => focusItem(0)); }}
  >{label}<span aria-hidden="true">⌄</span></button>
  {#if open}
    <div
      class="arcl-menu__items"
      role="menu"
      aria-label={label}
      tabindex="-1"
      onkeydown={handleKeydown}
      onfocusout={(event) => { if (!root.contains(event.relatedTarget as Node)) open = false; }}
    >
      {#each items as item (item.id)}
        <button
          type="button"
          role="menuitem"
          disabled={item.disabled}
          data-destructive={item.destructive || undefined}
          onclick={() => choose(item)}
        >{item.label}</button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .arcl-menu { position: relative; display: inline-block; }
  .arcl-menu__trigger { display: inline-flex; min-height: var(--arcl-control-height); align-items: center; gap: var(--arcl-space-2); padding-inline: var(--arcl-space-3); border: 1px solid var(--arcl-border); border-radius: var(--arcl-radius-md); color: var(--arcl-text); background: var(--arcl-surface); font-family: var(--arcl-font-ui); font-weight: 650; cursor: pointer; }
  .arcl-menu__trigger:hover { background: var(--arcl-surface-hover); }
  .arcl-menu__items { position: absolute; z-index: 20; top: calc(100% + var(--arcl-space-2)); right: 0; display: grid; min-width: 12rem; padding: var(--arcl-space-2); border: 1px solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); box-shadow: var(--arcl-shadow-overlay); }
  [role="menuitem"] { min-height: 2.5rem; padding-inline: var(--arcl-space-3); border: 0; border-radius: var(--arcl-radius-sm); color: var(--arcl-text); background: transparent; text-align: left; cursor: pointer; }
  [role="menuitem"]:hover, [role="menuitem"]:focus-visible { background: var(--arcl-surface-hover); }
  [role="menuitem"][data-destructive] { color: var(--arcl-danger); }
  [role="menuitem"]:disabled { opacity: 0.55; cursor: not-allowed; }
</style>
