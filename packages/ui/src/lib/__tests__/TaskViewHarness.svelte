<script lang="ts">
  import MarkdownEditor from "../components/MarkdownEditor.svelte";
  import PropertiesPanel from "../components/PropertiesPanel.svelte";

  let { readonly = true }: { readonly?: boolean } = $props();
  let open = $state(true);
  let edits = $state(0);
  let value = $state("# Title\n\nSome **bold** text.");
</script>

<div data-arcl-theme="light">
  <PropertiesPanel properties={[{ label: "Status", value: "Pending", status: "pending" }]} bind:open shortcut="]" />
  <MarkdownEditor {value} label="Description Markdown" {readonly} onchange={() => edits++} />
  <output>{edits} edits</output>
  <button type="button" onclick={() => (value = "Replaced from props.")}>Replace value</button>
</div>
