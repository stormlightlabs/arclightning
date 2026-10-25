<script module lang="ts">
  import type { Snippet } from "svelte";

  /** Props for the shared modal dialog. */
  export interface DialogProps {
    /**
     * Drop the visible header and its close button, and sit near the top of
     * the window, as a command menu does. The title still names the dialog
     * for assistive technology, and Escape still closes it.
     */
    bare?: boolean;
    children?: Snippet;
    description?: string;
    open?: boolean;
    onclose?: () => void;
    title: string;
  }
</script>

<script lang="ts">
  const generatedId = $props.id();
  let { bare = false, children, description, open = $bindable(false), onclose, title }: DialogProps = $props();
  const titleId = `${generatedId}-title`;
  const descriptionId = `${titleId}-description`;
  let dialog: HTMLDialogElement;
  /** What had focus when the dialog opened, which gets it back on close. */
  let opener: Element | null = null;

  /**
   * After a close, swallow the repeats of an Enter or Space still held from
   * inside the dialog, so they do not press the opener that gets focus back.
   * The first keyup or fresh key press ends it, so a new press acts as usual.
   */
  function swallowHeldKeys() {
    window.addEventListener("keydown", swallowHeldKey, true);
    window.addEventListener("keyup", swallowHeldKey, true);
  }

  function swallowHeldKey(event: KeyboardEvent) {
    if (event.type === "keydown" && event.repeat && (event.key === "Enter" || event.key === " ")) {
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    stopSwallowing();
  }

  function stopSwallowing() {
    window.removeEventListener("keydown", swallowHeldKey, true);
    window.removeEventListener("keyup", swallowHeldKey, true);
  }

  // Update `open` and focus as the dialog closes rather than on the `close`
  // event, which comes a task later, so a key pressed right after Escape
  // reaches the page with the dialog closed.
  function requestClose() {
    swallowHeldKeys();
    dialog.close();
    handleClose();
  }

  function handleClose() {
    // A `close` event queued before the dialog opened again, or that arrives
    // after the dialog unmounted, is stale.
    if (!dialog || dialog.open) return;
    if (open) swallowHeldKeys();
    if (dialog.contains(document.activeElement)) {
      if (opener instanceof HTMLElement && opener.isConnected) opener.focus();
      // Focusing `body`, or any opener that cannot take focus, leaves focus
      // in the dialog until its `close` event, so drop it here instead.
      if (dialog.contains(document.activeElement)) (document.activeElement as HTMLElement | null)?.blur();
    }
    const notify = open;
    open = false;
    if (notify) onclose?.();
  }

  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) {
      stopSwallowing();
      opener = document.activeElement;
      dialog.showModal();
    }
    if (!open && dialog.open) {
      swallowHeldKeys();
      dialog.close();
      handleClose();
    }
  });

  $effect(() => stopSwallowing);
</script>

<dialog
  bind:this={dialog}
  class="arcl-dialog"
  data-bare={bare || undefined}
  aria-labelledby={titleId}
  aria-describedby={description ? descriptionId : undefined}
  onclose={handleClose}
  oncancel={(event) => { event.preventDefault(); requestClose(); }}
  onkeydown={(event) => {
    // Close on Escape here, since the browser skips `cancel` for a repeated
    // close request without user activation and closes a task later.
    if (event.key === "Escape" && !event.defaultPrevented) { event.preventDefault(); requestClose(); }
  }}
  onclick={(event) => { if (event.target === dialog) requestClose(); }}
>
  {#if bare}
    <h2 class="arcl-dialog__hidden" id={titleId}>{title}</h2>
    {#if description}<p class="arcl-dialog__hidden" id={descriptionId}>{description}</p>{/if}
  {:else}
    <header>
      <div>
        <h2 id={titleId}>{title}</h2>
        {#if description}<p id={descriptionId}>{description}</p>{/if}
      </div>
      <button type="button" aria-label="Close dialog" onclick={requestClose}>×</button>
    </header>
  {/if}
  <div class="arcl-dialog__body">{@render children?.()}</div>
</dialog>

<style>
  .arcl-dialog { width: min(34rem, calc(100% - 2rem)); max-height: calc(100dvh - 2rem); padding: 0; border: 1px solid var(--arcl-border); border-radius: var(--arcl-radius-lg); color: var(--arcl-text); background: var(--arcl-surface); box-shadow: var(--arcl-shadow-overlay); animation: enter var(--arcl-duration-overlay) var(--arcl-ease-out); }
  .arcl-dialog::backdrop { background: var(--arcl-overlay); }
  header { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--arcl-space-4); padding: var(--arcl-space-5); border-bottom: 1px solid var(--arcl-border); }
  h2, p { margin: 0; }
  h2 { color: var(--arcl-heading); font: 650 var(--arcl-type-lg) / 1.25 var(--arcl-font-ui); }
  p { margin-top: var(--arcl-space-1); color: var(--arcl-text-muted); }
  header button { display: grid; width: 2.75rem; height: 2.75rem; place-items: center; border: 0; border-radius: var(--arcl-radius-control); color: var(--arcl-text); background: transparent; font-size: 1.5rem; cursor: pointer; }
  header button:hover { background: var(--arcl-surface-hover); }
  .arcl-dialog__body { padding: var(--arcl-space-5); }
  .arcl-dialog[data-bare] { width: min(40rem, calc(100% - 2rem)); margin-top: min(12vh, 7.5rem); }
  .arcl-dialog[data-bare] .arcl-dialog__body { padding: 0; }
  .arcl-dialog__hidden { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  @media (prefers-reduced-motion: reduce) { .arcl-dialog { animation: none; } }
  @keyframes enter { from { opacity: 0; transform: translateY(-0.5rem); } }
</style>
