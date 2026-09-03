<script module lang="ts">
  import type { Snippet } from "svelte";
  import type { HTMLButtonAttributes } from "svelte/elements";

  /** Visual emphasis available to shared actions. */
  export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";

  /** Props for an Arc Lightning button. */
  export interface ButtonProps extends Omit<HTMLButtonAttributes, "children"> {
    busy?: boolean;
    children?: Snippet;
    label?: string;
    variant?: ButtonVariant;
  }
</script>

<script lang="ts">
  let {
    busy = false,
    children,
    class: className = "",
    disabled = false,
    label,
    type = "button",
    variant = "secondary",
    ...rest
  }: ButtonProps = $props();
</script>

<button
  {...rest}
  aria-busy={busy}
  class={["arcl-button", className]}
  data-variant={variant}
  disabled={disabled || busy}
  {type}
>
  {#if busy}<span class="arcl-button__spinner" aria-hidden="true"></span>{/if}
  <span>{#if children}{@render children()}{:else}{label}{/if}</span>
</button>

<style>
  .arcl-button {
    display: inline-flex;
    min-height: var(--arcl-control-height);
    align-items: center;
    justify-content: center;
    gap: var(--arcl-space-2);
    padding-inline: var(--arcl-space-4);
    border: var(--arcl-border-width) solid var(--arcl-border-strong);
    border-radius: var(--arcl-radius-md);
    color: var(--arcl-text);
    background: var(--arcl-surface);
    box-shadow: var(--arcl-shadow-control);
    font-family: var(--arcl-font-ui);
    font-weight: 650;
    cursor: pointer;
    transition:
      background-color var(--arcl-duration-fast) var(--arcl-ease-out),
      transform var(--arcl-duration-fast) var(--arcl-ease-out);
  }

  .arcl-button:hover:not(:disabled) { background: var(--arcl-surface-hover); }
  .arcl-button:active:not(:disabled) { transform: scale(0.98); }
  .arcl-button:disabled { opacity: 0.58; cursor: not-allowed; }
  .arcl-button[data-variant="primary"] { color: var(--arcl-on-accent); background: var(--arcl-accent); border-color: var(--arcl-accent); }
  .arcl-button[data-variant="primary"]:hover:not(:disabled) { background: var(--arcl-accent-hover); }
  .arcl-button[data-variant="ghost"] { border-color: transparent; background: transparent; box-shadow: none; }
  .arcl-button[data-variant="danger"] { color: white; background: var(--arcl-danger); border-color: var(--arcl-danger); }

  .arcl-button__spinner {
    width: 1rem;
    height: 1rem;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 700ms linear infinite;
  }

  @keyframes spin { to { transform: rotate(1turn); } }
</style>
