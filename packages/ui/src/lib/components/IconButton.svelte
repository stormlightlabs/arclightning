<script module lang="ts">
  import type { HTMLButtonAttributes } from "svelte/elements";
  import type { IconName } from "../icons";

  /** Props for a compact icon-only action with a required accessible name. */
  export interface IconButtonProps extends HTMLButtonAttributes {
    label: string;
    name: IconName;
    selected?: boolean;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  let { label, name, selected = false, type = "button", ...rest }: IconButtonProps = $props();
</script>

<button {...rest} class="arcl-icon-button" aria-label={label} aria-pressed={selected} {type}>
  <Icon {name} />
</button>

<style>
  .arcl-icon-button { display: inline-grid; width: var(--arcl-control-height); height: var(--arcl-control-height); place-items: center; padding: 0; border: 1px solid transparent; border-radius: var(--arcl-radius-md); color: var(--arcl-text); background: transparent; cursor: pointer; }
  .arcl-icon-button:hover { background: var(--arcl-surface-hover); }
  .arcl-icon-button[aria-pressed="true"] { color: var(--arcl-on-accent); background: var(--arcl-accent); }
  .arcl-icon-button:disabled { opacity: 0.58; cursor: not-allowed; }
</style>
