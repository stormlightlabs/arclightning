<script lang="ts">
  import Select from "../components/Select.svelte";
  import SelectMenu from "../components/SelectMenu.svelte";

  let { theme = "light", end = false }: { theme?: "light" | "dark"; end?: boolean } = $props();
  let value = $state("");
  let changes = $state<string[]>([]);
  const options = [
    { value: "", label: "Any" },
    { value: "pending", label: "Pending" },
    { value: "parked", label: "Parked", disabled: true },
    { value: "planned", label: "Planned" },
    { value: "completed", label: "Completed" },
  ];
</script>

<div data-arcl-theme={theme} style:display="flex" style:justify-content={end ? "flex-end" : "flex-start"}>
  <button type="button">Outside</button>
  <SelectMenu label="Status" {options} bind:value onchange={(next) => (changes = [...changes, next])} />
  <Select label="Priority" options={[{ value: "low", label: "Low" }, { value: "high", label: "High" }]} />
  <output data-testid="value">{value}</output>
  <output data-testid="changes">{changes.join(",")}</output>
</div>
