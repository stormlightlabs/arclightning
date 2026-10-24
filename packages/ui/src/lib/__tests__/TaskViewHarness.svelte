<script lang="ts">
  import MarkdownEditor from "../components/MarkdownEditor.svelte";
  import PropertiesPanel from "../components/PropertiesPanel.svelte";

  let { readonly = true }: { readonly?: boolean } = $props();
  let open = $state(true);
  let edits = $state(0);
</script>

<div data-arcl-theme="light">
  <PropertiesPanel properties={[{ label: "Status", value: "Pending", status: "pending" }]} bind:open shortcut="]" />
  <MarkdownEditor value={"# Title\n\nSome **bold** text."} label="Description Markdown" {readonly} onchange={() => edits++} />
  <output>{edits} edits</output>
</div>
