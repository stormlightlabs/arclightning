<script module lang="ts">
  import type { HTMLInputAttributes, HTMLTextareaAttributes } from "svelte/elements";

  /** Props for a labelled single- or multi-line text field. */
  export interface FieldProps extends Omit<HTMLInputAttributes & HTMLTextareaAttributes, "value"> {
    description?: string;
    error?: string;
    label: string;
    multiline?: boolean;
    value?: string;
  }
</script>

<script lang="ts">
  const generatedId = $props.id();
  let {
    description,
    error,
    id = generatedId,
    label,
    multiline = false,
    value = $bindable(""),
    ...rest
  }: FieldProps = $props();
  let descriptionId = $derived(`${id}-description`);
  let errorId = $derived(`${id}-error`);
  let describedBy = $derived([description ? descriptionId : "", error ? errorId : ""].filter(Boolean).join(" ") || undefined);
</script>

<label class="arcl-field" for={id}>
  <span class="arcl-field__label">{label}</span>
  {#if description}<span class="arcl-field__description" id={descriptionId}>{description}</span>{/if}
  {#if multiline}
    <textarea
      {...rest}
      {id}
      bind:value
      aria-describedby={describedBy}
      aria-invalid={error ? "true" : undefined}
    ></textarea>
  {:else}
    <input
      {...rest}
      {id}
      bind:value
      aria-describedby={describedBy}
      aria-invalid={error ? "true" : undefined}
    />
  {/if}
  {#if error}<span class="arcl-field__error" id={errorId}>{error}</span>{/if}
</label>

<style>
  .arcl-field { display: grid; gap: var(--arcl-space-2); color: var(--arcl-text); }
  .arcl-field__label { font-family: var(--arcl-font-ui); font-weight: 650; }
  .arcl-field__description { color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); }
  .arcl-field__error { color: var(--arcl-danger); font-size: var(--arcl-type-sm); font-weight: 600; }
  input,
  textarea {
    width: 100%;
    min-height: var(--arcl-control-height);
    padding: var(--arcl-space-3);
    border: 1px solid var(--arcl-border);
    border-radius: var(--arcl-radius-md);
    color: var(--arcl-text);
    background: var(--arcl-surface);
  }
  textarea { min-height: 7rem; resize: vertical; }
  [aria-invalid="true"] { border-color: var(--arcl-danger); box-shadow: 0 0 0 1px var(--arcl-danger); }
  ::placeholder { color: var(--arcl-text-muted); opacity: 0.78; }
</style>
