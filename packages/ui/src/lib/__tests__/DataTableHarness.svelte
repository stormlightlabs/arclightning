<script lang="ts">
  import DataTable, { type DataTableColumn } from "../components/DataTable.svelte";

  interface Item { id: string; title: string; note: string; }
  let { items, width = "40rem" }: { items: Item[]; width?: string } = $props();
  let selectedId = $state<string | null>(null);
  const columns: DataTableColumn[] = [
    { id: "title", label: "Title" },
    { id: "note", label: "Note", width: "10rem", hideNarrow: true },
  ];
</script>

<div data-arcl-theme="light" style:width>
  <button type="button" onclick={() => (selectedId = "b")}>Select second</button>
  <div class="box" style="height: 8rem; display: grid;">
    <DataTable rows={items} {columns} rowId={(item) => item.id} {selectedId} onselect={(id) => (selectedId = id)} label="Items">
      {#snippet cell(item, column)}{column.id === "title" ? item.title : item.note}{/snippet}
    </DataTable>
  </div>
  <output>{selectedId}</output>
</div>
