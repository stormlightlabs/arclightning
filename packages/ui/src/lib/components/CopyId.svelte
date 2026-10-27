<script module lang="ts">
  /** Props for a record ID that copies itself to the clipboard. */
  export type CopyIdProps = {
    id: string;
    /** The button's accessible name. */
    label?: string;
    class?: string;
  };

  /** What the live region says after a copy that worked. */
  export const COPIED = "Copied";
  /** What it says when the clipboard is missing or refuses, and the ID is selected instead. */
  export const COPY_BY_HAND = "Selected. Copy it by hand.";

  /** How long the message shows, in ms. */
  const MESSAGE_MS = 2500;
</script>

<script lang="ts">
  import { tick } from "svelte";
  import Icon from "./Icon.svelte";

  let { id, label = "Copy ID", class: className = "" }: CopyIdProps = $props();
  let text: HTMLElement;
  let message = $state("");
  let timer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => () => clearTimeout(timer));

  /**
   * Write the ID to the clipboard. `navigator.clipboard` needs a secure
   * context, and a host such as the claude.ai artifact iframe may refuse the
   * write, so either failure selects the ID for a manual copy instead.
   */
  async function copy(event: MouseEvent) {
    // Keep the click from the card, row or link around the ID.
    event.stopPropagation();
    clearTimeout(timer);
    message = "";
    let copied = false;
    try {
      if (typeof navigator.clipboard?.writeText === "function") {
        await navigator.clipboard.writeText(id);
        copied = true;
      }
    } catch {
      copied = false;
    }
    if (!copied) select();
    // Clear first so a repeated message is announced again.
    await tick();
    message = copied ? COPIED : COPY_BY_HAND;
    timer = setTimeout(() => (message = ""), MESSAGE_MS);
  }

  function select() {
    const selection = window.getSelection();
    if (!selection) return;
    const range = document.createRange();
    range.selectNodeContents(text);
    selection.removeAllRanges();
    selection.addRange(range);
  }
</script>

<span class={["arcl-copy-id", className]}>
  <!-- The button beside it is the keyboard route; a click on the ID is a shortcut. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <code bind:this={text} title="Click to copy" onclick={copy}>{id}</code>
  <button type="button" aria-label={label} title={label} onclick={copy}>
    <Icon name="copy" size="0.875rem" />
  </button>
  <span class="arcl-copy-id__message" role="status">{message}</span>
</span>

<style>
  .arcl-copy-id {
    display: inline-flex;
    min-width: 0;
    max-width: 100%;
    align-items: center;
    color: var(--arcl-text-muted);
  }
  code {
    overflow: hidden;
    font: inherit;
    font-family: var(--arcl-font-code);
    text-overflow: ellipsis;
    white-space: nowrap;
    cursor: copy;
  }
  button {
    display: inline-grid;
    flex: none;
    width: 1.5rem;
    height: 1.5rem;
    place-items: center;
    margin-inline-start: var(--arcl-space-1);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--arcl-radius-control);
    color: inherit;
    background: transparent;
    cursor: pointer;
  }
  button:hover {
    color: var(--arcl-text);
    background: var(--arcl-surface-hover);
  }
  .arcl-copy-id__message {
    flex: none;
    font-family: var(--arcl-font-ui);
    font-size: var(--arcl-type-xs);
  }
  .arcl-copy-id__message:not(:empty) {
    margin-inline-start: var(--arcl-space-1);
  }
</style>
