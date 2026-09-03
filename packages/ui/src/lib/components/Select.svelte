<script module lang="ts">
  import type { HTMLSelectAttributes } from "svelte/elements";

  /** A value and label displayed by the shared selector. */
  export interface SelectOption { value: string; label: string; disabled?: boolean; }
  /** Props for a labelled native selector. */
  export interface SelectProps extends Omit<HTMLSelectAttributes, "value"> {
    label: string;
    options: SelectOption[];
    value?: string;
  }
</script>

<script lang="ts">
  const generatedId = $props.id();
  let { id = generatedId, label, options, value = $bindable(""), ...rest }: SelectProps = $props();
</script>

<label class="arcl-select" for={id}>
  <span>{label}</span>
  <select {...rest} {id} bind:value>
    {#each options as option (option.value)}
      <option value={option.value} disabled={option.disabled}>{option.label}</option>
    {/each}
  </select>
</label>

<style>
  .arcl-select { display: grid; gap: var(--arcl-space-2); color: var(--arcl-text); font-family: var(--arcl-font-ui); font-weight: 650; }
  select { min-height: var(--arcl-control-height); padding-inline: var(--arcl-space-3); border: 1px solid var(--arcl-border); border-radius: var(--arcl-radius-md); color: var(--arcl-text); background: var(--arcl-surface); font-family: var(--arcl-font-body); font-weight: 400; }
</style>
