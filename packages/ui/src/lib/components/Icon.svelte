<script module lang="ts">
  import type { HTMLAttributes } from "svelte/elements";
  import type { IconName } from "../icons";

  /** Props for an Iconify-backed icon named by product meaning. */
  export interface IconProps extends Omit<HTMLAttributes<HTMLSpanElement>, "children"> {
    color?: string;
    label?: string;
    name: IconName;
    size?: number | string;
  }
</script>

<script lang="ts">
  import { ICONS } from "../icons";
  let { class: className = "", color, label, name, size = "1.25rem", ...rest }: IconProps = $props();
  let resolvedSize = $derived(typeof size === "number" ? `${size}px` : size);
</script>

<span
  {...rest}
  aria-hidden={label ? undefined : "true"}
  aria-label={label}
  class={["arcl-icon", ICONS[name], className]}
  role={label ? "img" : undefined}
  style:--arcl-icon-size={resolvedSize}
  style:color
></span>

<style>
  .arcl-icon { width: var(--arcl-icon-size); height: var(--arcl-icon-size); }
</style>
