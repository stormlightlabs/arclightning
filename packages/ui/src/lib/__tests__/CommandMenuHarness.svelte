<script lang="ts">
  import CommandMenu, { type CommandGroup } from "../components/CommandMenu.svelte";

  let open = $state(false);
  let query = $state("");
  let context = $state<string | undefined>("T10.1 · Add the skill");
  let ran = $state<string[]>([]);
  const all: CommandGroup[] = [
    {
      id: "selection",
      label: "Actions on T10.1",
      items: [
        { id: "start", label: "Start", keys: ["S"] },
        { id: "park", label: "Park", keys: ["P"] },
      ],
    },
    { id: "go", label: "Go to", items: [{ id: "work", label: "Work", keys: ["G", "W"] }] },
  ];
  let groups = $derived(
    all
      .filter((group) => context !== undefined || group.id !== "selection")
      .map((group) => ({
        ...group,
        items: group.items.filter((item) => item.label.toLowerCase().includes(query.toLowerCase())),
      }))
      .filter((group) => group.items.length),
  );
</script>

<button type="button" onclick={() => (open = true)}>Open menu</button>
<CommandMenu
  bind:open
  bind:query
  {groups}
  {context}
  onclearcontext={() => (context = undefined)}
  onrun={(id) => (ran = [...ran, id])}
/>
<output data-testid="ran">{ran.join(",")}</output>
