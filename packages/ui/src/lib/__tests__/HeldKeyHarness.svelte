<script lang="ts">
  import Dialog from "../components/Dialog.svelte";

  let open = $state(false);
  let presses = $state(0);
</script>

<!-- A dialog whose button closes it, opened from a button that counts presses. -->
<button type="button" onclick={() => ((presses += 1), (open = true))}>Open</button>
<Dialog bind:open bare title="Held key">
  <button type="button" onclick={() => (open = false)}>Choose</button>
</Dialog>
<output>{presses} presses</output>
