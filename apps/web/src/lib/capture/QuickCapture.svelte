<script module lang="ts">
  import type { Capture, ChangeWarning, Task } from "$lib/data";

  /** Props for the quick capture dialog. */
  export interface QuickCaptureProps {
    open?: boolean;
    /** Whether `Mod` is `⌘`, for the key hints. */
    mac?: boolean;
    /** Called after `Enter` files a capture, with the warnings its syncs gave. */
    oncaptured?: (capture: Capture, warnings: ChangeWarning[]) => void;
    /** Called after `Mod+Enter` files a task, with the warnings its syncs gave. */
    oncreated?: (task: Task, warnings: ChangeWarning[]) => void;
  }
</script>

<script lang="ts">
  import { Button, Dialog, Feedback } from "@arclightning/ui";
  import { dataSource } from "$lib/data";
  import { failureOf, type ActionFailure } from "$lib/task";
  import { captureOf, describeToken, parseCapture, taskOf } from "./tokens";

  let { open = $bindable(false), mac = false, oncaptured, oncreated }: QuickCaptureProps = $props();

  let text = $state("");
  let busy = $state(false);
  let failure = $state<ActionFailure | null>(null);
  let parsed = $derived(parseCapture(text));
  let mod = $derived(mac ? "⌘" : "Ctrl");

  // Each opening starts empty.
  $effect(() => {
    if (open) {
      text = "";
      failure = null;
    }
  });

  async function file(asTask: boolean) {
    if (busy || parsed.line === "") return;
    busy = true;
    failure = null;
    try {
      if (asTask) {
        const tasks = parsed.tokens.some((token) => token.kind !== "priority") ? await dataSource().taskIndex() : [];
        const result = taskOf(parsed, tasks);
        if ("problems" in result) {
          const names = result.problems.map(({ ref, problem }) => `\`${ref}\` ${problem === "missing" ? "names no task" : "names more than one task"}`);
          failure = { code: "invalid_reference", message: names.join("; ") };
          return;
        }
        const { value: task, warnings } = await dataSource().createTask(result.task);
        open = false;
        oncreated?.(task, warnings);
      } else {
        const { value: capture, warnings } = await dataSource().createCapture(captureOf(parsed));
        open = false;
        oncaptured?.(capture, warnings);
      }
    } catch (error) {
      failure = failureOf(error);
    } finally {
      busy = false;
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Enter" || event.shiftKey || event.altKey || event.isComposing) return;
    event.preventDefault();
    void file(mac ? event.metaKey : event.ctrlKey);
  }
</script>

<Dialog bind:open title="Quick capture" description="The first line is the title; Shift+Enter starts the body.">
  <form class="capture" onsubmit={(event) => (event.preventDefault(), file(false))}>
    <!-- svelte-ignore a11y_autofocus -->
    <textarea
      aria-label="Capture"
      aria-describedby="capture-tokens"
      rows="3"
      placeholder="Show unblocked tasks after completing !high after:T18.4"
      autofocus
      bind:value={text}
      {onkeydown}
    ></textarea>
    <p class="tokens" id="capture-tokens" aria-live="polite">
      {#if parsed.tokens.length}
        {mod}+Enter files a task: {parsed.tokens.map(describeToken).join(", ")}.
      {:else}
        <code>!high</code> sets the priority, <code>^T18</code> the parent and <code>after:T18.4</code> a blocker when {mod}+Enter files a task.
      {/if}
    </p>
    <div class="buttons">
      <Button type="submit" variant="primary" label="Capture" busy={busy} disabled={busy || parsed.line === ""} />
      <Button label="Create task" disabled={busy || parsed.title === ""} onclick={() => file(true)} />
      <span class="keys" aria-hidden="true"><kbd>↵</kbd> capture <kbd>{mod}</kbd><kbd>↵</kbd> task</span>
    </div>
    {#if failure !== null}
      <Feedback tone="danger" title="Could not file this" message={`${failure.message} (${failure.code})`} />
    {/if}
  </form>
</Dialog>

<style>
  .capture { display: grid; gap: var(--arcl-space-3); }
  textarea { width: 100%; box-sizing: border-box; padding: var(--arcl-space-2) var(--arcl-space-3); border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); color: var(--arcl-text); background: var(--arcl-surface); font: var(--arcl-type-base) / 1.5 var(--arcl-font-body); resize: vertical; }
  textarea:focus { border-color: var(--arcl-focus); outline: var(--arcl-focus-width) solid color-mix(in srgb, var(--arcl-focus) 30%, transparent); }
  .tokens { margin: 0; color: var(--arcl-text-muted); font-size: var(--arcl-type-sm); }
  .tokens code { padding: 0 0.25rem; border-radius: 4px; background: var(--arcl-surface-subtle); font: var(--arcl-type-xs) var(--arcl-font-code); }
  .buttons { display: flex; flex-wrap: wrap; align-items: center; gap: var(--arcl-space-2); }
  .keys { display: inline-flex; align-items: center; gap: var(--arcl-space-1); margin-inline-start: auto; color: var(--arcl-text-muted); font-size: var(--arcl-type-xs); }
  kbd { display: inline-flex; min-width: 1.125rem; height: 1.125rem; align-items: center; justify-content: center; padding: 0 0.25rem; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: 4px; background: var(--arcl-surface); font: 500 var(--arcl-type-xs) / 1 var(--arcl-font-ui); }
</style>
