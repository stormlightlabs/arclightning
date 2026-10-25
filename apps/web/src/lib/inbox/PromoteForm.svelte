<script module lang="ts">
  import type { SelectOption } from "@arclightning/ui";
  import type { PromotionForm } from "./triage";

  /** Props for the form that promotes the selected capture. */
  export interface PromoteFormProps {
    form: PromotionForm;
    /** The phases a task can go in, after a `None` option. */
    phases?: SelectOption[];
    /** The tasks a task can go under, after a `None` option. */
    parents?: SelectOption[];
    busy?: boolean;
    onsubmit: () => void;
    /** Called by Cancel. The view closes the form on Escape too, while it holds no edits. */
    oncancel: () => void;
  }
</script>

<script lang="ts">
  import { Button, Field, SelectMenu } from "@arclightning/ui";
  import { PRIORITIES } from "$lib/work";
  import { TRIAGE_LABELS } from "./triage";

  let { form = $bindable(), phases = [], parents = [], busy = false, onsubmit, oncancel }: PromoteFormProps = $props();

  const none: SelectOption = { value: "", label: "None" };
  const priorities = PRIORITIES.map((priority) => ({ value: priority, label: priority[0].toUpperCase() + priority.slice(1) }));
  const submitLabel = { task: "Create task", spec: "Create spec", note: "Create note" };
</script>

<form class="promote" aria-labelledby="promote-title" onsubmit={(event) => (event.preventDefault(), onsubmit())}>
  <h3 id="promote-title">{TRIAGE_LABELS[form.to]}</h3>
  <Field label="Title" bind:value={form.title} required />
  {#if form.to === "task"}
    <div class="grid">
      <SelectMenu label="Priority" options={priorities} value={form.priority} onchange={(value) => form.to === "task" && (form.priority = value as typeof form.priority)} />
      <SelectMenu label="Milestone" options={[none, ...phases]} value={form.phase} onchange={(value) => form.to === "task" && (form.phase = value)} />
      <SelectMenu label="Parent" options={[none, ...parents]} value={form.parent} onchange={(value) => form.to === "task" && (form.parent = value)} />
    </div>
    {#each form.notes as note (note)}<p class="note">{note}</p>{/each}
    <p class="hint">The task gets the capture's body.</p>
  {:else if form.to === "spec"}
    <Field label="Acceptance criteria" description="Markdown; you can add them later." multiline bind:value={form.acceptance} />
    <p class="hint">The spec gets the capture's body. Plan it from Planning.</p>
  {:else}
    <p class="hint">The note gets the capture's body.</p>
  {/if}
  <div class="buttons">
    <Button type="submit" variant="primary" label={submitLabel[form.to]} {busy} disabled={busy} />
    <Button label="Cancel" variant="ghost" disabled={busy} onclick={oncancel} />
  </div>
</form>

<style>
  .promote { display: grid; gap: var(--arcl-space-3); max-width: 40rem; padding: var(--arcl-space-4); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); }
  h3 { margin: 0; color: var(--arcl-heading); font: 600 var(--arcl-type-base) / 1.3 var(--arcl-font-ui); }
  .grid { display: flex; flex-wrap: wrap; gap: var(--arcl-space-3); font-size: var(--arcl-type-sm); }
  .note { margin: 0; padding: var(--arcl-space-2) var(--arcl-space-3); border-inline-start: 4px solid var(--arcl-warning); border-radius: var(--arcl-radius-sm); background: var(--arcl-warning-surface); font-size: var(--arcl-type-sm); }
  .hint { margin: 0; color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); }
  .buttons { display: flex; gap: var(--arcl-space-2); }
</style>
