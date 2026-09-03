<script lang="ts">
  import Button from "../Button.svelte";
  import Dialog from "../Dialog.svelte";
  let open = $state(false);
</script>

<div class="preview" data-arcl-theme="light">
  <Button label="Review conflict" onclick={() => open = true} />
  <Dialog bind:open title="Review conflict" description="No files change until you choose a side.">
    <p>The database and workspace changed differently from their common base.</p>
    <Button variant="primary" label="Keep database" />
  </Dialog>
</div>

<style>.preview { min-height: 12rem; padding: var(--arcl-space-6); background: var(--arcl-canvas); } p { max-width: 40ch; }</style>
