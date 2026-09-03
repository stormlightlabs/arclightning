<script module lang="ts">
  import type { Snippet } from "svelte";

  /** Props for the shared modal dialog. */
  export interface DialogProps {
    children?: Snippet;
    description?: string;
    open?: boolean;
    onclose?: () => void;
    title: string;
  }
</script>

<script lang="ts">
  const generatedId = $props.id();
  let { children, description, open = $bindable(false), onclose, title }: DialogProps = $props();
  const titleId = `${generatedId}-title`;
  const descriptionId = `${titleId}-description`;
  let dialog: HTMLDialogElement;

  function requestClose() {
    dialog.close();
  }

  function handleClose() {
    const notify = open;
    open = false;
    if (notify) onclose?.();
  }

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    if (!open && dialog.open) dialog.close();
  });
</script>

<dialog
  bind:this={dialog}
  class="arcl-dialog"
  aria-labelledby={titleId}
  aria-describedby={description ? descriptionId : undefined}
  onclose={handleClose}
  onclick={(event) => { if (event.target === dialog) requestClose(); }}
>
  <header>
    <div>
      <h2 id={titleId}>{title}</h2>
      {#if description}<p id={descriptionId}>{description}</p>{/if}
    </div>
    <button type="button" aria-label="Close dialog" onclick={requestClose}>×</button>
  </header>
  <div class="arcl-dialog__body">{@render children?.()}</div>
</dialog>

<style>
  .arcl-dialog { width: min(34rem, calc(100% - 2rem)); max-height: calc(100dvh - 2rem); padding: 0; border: 1px solid var(--arcl-border); border-radius: var(--arcl-radius-lg); color: var(--arcl-text); background: var(--arcl-surface); box-shadow: var(--arcl-shadow-overlay); animation: enter var(--arcl-duration-overlay) var(--arcl-ease-out); }
  .arcl-dialog::backdrop { background: var(--arcl-overlay); }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--arcl-space-4); padding: var(--arcl-space-5); border-bottom: 1px solid var(--arcl-border); }
  h2, p { margin: 0; }
  h2 { color: var(--arcl-heading); font: 650 var(--arcl-type-lg) / 1.25 var(--arcl-font-ui); }
  p { margin-top: var(--arcl-space-1); color: var(--arcl-text-muted); }
  header button { display: grid; width: 2.75rem; height: 2.75rem; place-items: center; border: 0; border-radius: var(--arcl-radius-md); color: var(--arcl-text); background: transparent; font-size: 1.5rem; cursor: pointer; }
  header button:hover { background: var(--arcl-surface-hover); }
  .arcl-dialog__body { padding: var(--arcl-space-5); }
  @keyframes enter { from { opacity: 0; transform: translateY(-0.5rem); } }
</style>
