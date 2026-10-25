<script lang="ts">
  import Dialog from "../components/Dialog.svelte";

  let open = $state(true);
</script>

<!-- Opens with nothing focused, as a dialog a shortcut opens does. -->
<Dialog bind:open title="Opened by a shortcut">
  <!-- svelte-ignore a11y_autofocus -->
  <input aria-label="Inside" autofocus />
</Dialog>
<output>{open ? "open" : "closed"}</output>
