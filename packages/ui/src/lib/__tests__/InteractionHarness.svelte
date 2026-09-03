<script lang="ts">
  import { createRawSnippet } from "svelte";
  import Button from "../components/Button.svelte";
  import Dialog from "../components/Dialog.svelte";
  import Field from "../components/Field.svelte";
  import Menu from "../components/Menu.svelte";
  import Tabs, { type TabItem } from "../components/Tabs.svelte";

  let dialogOpen = $state(false);
  let name = $state("");
  let result = $state("No action selected");

  const first = createRawSnippet(() => ({ render: () => "<p>First panel</p>" }));
  const second = createRawSnippet(() => ({ render: () => "<p>Second panel</p>" }));
  const tabs: TabItem[] = [
    { id: "first", label: "First", panel: first },
    { id: "second", label: "Second", panel: second },
  ];
</script>

<div data-arcl-theme="light">
  <Button label="Open dialog" onclick={() => dialogOpen = true} />
  <Field label="Name" bind:value={name} />
  <output>{name}</output>
  <Menu items={[{ id: "promote", label: "Promote to task" }, { id: "disabled", label: "Archive", disabled: true }]} onselect={(item) => result = item.label} />
  <output>{result}</output>
  <Tabs label="Test views" items={tabs} />
  <Dialog bind:open={dialogOpen} title="Confirm promotion"><Button label="Done" onclick={() => dialogOpen = false} /></Dialog>
</div>
