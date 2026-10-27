<script module lang="ts">
  import type { IconName } from "../icons";

  /** One command the menu lists. */
  export interface CommandItem {
    /** Unique across the menu; {@link CommandMenuProps.onrun} receives it. */
    id: string;
    label: string;
    /** Muted text after the label, such as a task's status. */
    detail?: string;
    /** The keys that run the command outside the menu, shown as key caps. */
    keys?: string[];
    icon?: IconName;
  }

  /** A titled run of commands, such as the actions on the selected task. */
  export interface CommandGroup {
    id: string;
    label: string;
    items: CommandItem[];
  }

  /** Props for the command menu: a search field over grouped commands in a dialog. */
  export interface CommandMenuProps {
    /** Whether the menu is open. */
    open?: boolean;
    /** The search text. The caller filters `groups` with it. */
    query?: string;
    /** The groups to show, already filtered, in order. */
    groups: CommandGroup[];
    /** A chip naming what the first group acts on, such as the selected task. */
    context?: string;
    /** Called when Backspace in an empty search field removes the chip. */
    onclearcontext?: () => void;
    /** Called with the chosen command's ID after the menu closes. */
    onrun: (id: string) => void;
    onclose?: () => void;
    /** The dialog's accessible name. */
    label?: string;
    placeholder?: string;
    /** What the menu says when no command matches. */
    empty?: string;
  }
</script>

<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import Icon from "./Icon.svelte";
  import { listboxKey } from "../listbox";

  const id = $props.id();
  let {
    open = $bindable(false),
    query = $bindable(""),
    groups,
    context,
    onclearcontext,
    onrun,
    onclose,
    label = "Command menu",
    placeholder = "Type a command or search tasks",
    empty = "No command or task matches.",
  }: CommandMenuProps = $props();

  let active = $state(0);
  let listbox = $state<HTMLDivElement>();

  let items = $derived(groups.flatMap((group) => group.items));
  let listId = $derived(`${id}-listbox`);
  const optionId = (index: number) => `${id}-option-${index}`;
  /** The flat index of each group's first item. */
  let starts = $derived(
    groups.reduce<number[]>(
      (all, group, index) => [...all, index === 0 ? 0 : all[index - 1] + groups[index - 1].items.length],
      [],
    ),
  );

  // A new search or a new list starts at the top.
  $effect(() => {
    void query;
    void items;
    active = 0;
  });

  $effect(() => {
    if (open) listbox?.querySelector(`#${CSS.escape(optionId(active))}`)?.scrollIntoView({ block: "nearest" });
  });

  function run(index: number) {
    const item = items[index];
    if (!item) return;
    open = false;
    onrun(item.id);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Backspace" && query === "" && context !== undefined && onclearcontext) {
      event.preventDefault();
      onclearcontext();
      return;
    }
    // The field keeps Home, End, Space and typed characters; the list takes
    // the keys that move through it and Enter.
    if (!["ArrowDown", "ArrowUp", "PageDown", "PageUp", "Enter"].includes(event.key)) return;
    const step = listboxKey(event, { open: true, active, searching: false }, items);
    if (step === null) return;
    event.preventDefault();
    if (step.commit) run(step.active);
    else active = step.active;
  }

  /** The index of the option `target` sits in, or `null` outside any option. */
  function optionAt(target: EventTarget | null): number | null {
    const option = (target as Element | null)?.closest?.<HTMLElement>("[role=option]");
    return option ? Number(option.dataset.index) : null;
  }

  /** `text` split around the first case-insensitive match of the search. */
  function highlight(text: string): [string, string, string] {
    const needle = query.trim().toLowerCase();
    const at = needle === "" ? -1 : text.toLowerCase().indexOf(needle);
    if (at === -1) return [text, "", ""];
    return [text.slice(0, at), text.slice(at, at + needle.length), text.slice(at + needle.length)];
  }
</script>

<Dialog bind:open title={label} bare {onclose}>
  <div class="arcl-command-menu">
    {#if context !== undefined}
      <div class="arcl-command-menu__context"><span>{context}</span></div>
    {/if}
    <div class="arcl-command-menu__search">
      <Icon name="search" size="1rem" />
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="text"
        role="combobox"
        aria-label={label}
        aria-controls={listId}
        aria-expanded="true"
        aria-autocomplete="list"
        aria-activedescendant={items[active] ? optionId(active) : undefined}
        autocomplete="off"
        spellcheck="false"
        autofocus
        {placeholder}
        bind:value={query}
        {onkeydown} />
    </div>
    <div
      class="arcl-command-menu__list"
      role="listbox"
      id={listId}
      aria-label="Results"
      tabindex="-1"
      bind:this={listbox}
      onmousedown={(event) => event.preventDefault()}
      {onkeydown}
      onpointermove={(event) => {
        const index = optionAt(event.target);
        if (index !== null) active = index;
      }}
      onclick={(event) => {
        const index = optionAt(event.target);
        if (index !== null) run(index);
      }}>
      {#each groups as group, groupIndex (group.id)}
        <div role="group" aria-labelledby={`${id}-group-${groupIndex}`}>
          <div class="arcl-command-menu__group" id={`${id}-group-${groupIndex}`} role="presentation">{group.label}</div>
          {#each group.items as item, itemIndex (item.id)}
            {@const index = starts[groupIndex] + itemIndex}
            {@const [before, match, after] = highlight(item.label)}
            <div
              class="arcl-command-menu__option"
              role="option"
              id={optionId(index)}
              tabindex="-1"
              aria-selected={index === active}
              data-index={index}>
              <Icon name={item.icon ?? "arrowRight"} size="1rem" />
              <span class="arcl-command-menu__label"
                >{before}<b>{match}</b>{after}{#if item.detail}<span class="arcl-command-menu__detail">
                    · {item.detail}</span
                  >{/if}</span>
              {#if item.keys?.length}
                <span class="arcl-command-menu__keys"
                  >{#each item.keys as key, keyIndex (keyIndex)}<kbd>{key}</kbd>{/each}</span>
              {/if}
            </div>
          {/each}
        </div>
      {:else}
        <p class="arcl-command-menu__empty">{empty}</p>
      {/each}
    </div>
    <div class="arcl-command-menu__hints" aria-hidden="true">
      <span><kbd>↑</kbd><kbd>↓</kbd> move</span>
      <span><kbd>↵</kbd> run</span>
      {#if context !== undefined && onclearcontext}<span><kbd>⌫</kbd> clear {context}</span>{/if}
      <span class="arcl-command-menu__end"><kbd>esc</kbd> close</span>
    </div>
  </div>
</Dialog>

<style>
  .arcl-command-menu {
    display: grid;
    font-family: var(--arcl-font-ui);
  }
  .arcl-command-menu__context {
    padding: var(--arcl-space-3) var(--arcl-space-4) 0;
  }
  .arcl-command-menu__context span {
    display: inline-flex;
    max-width: 100%;
    align-items: center;
    min-height: 1.375rem;
    padding: 0 var(--arcl-space-2);
    overflow: hidden;
    border-radius: 999px;
    color: var(--arcl-text);
    background: var(--arcl-surface-subtle);
    font-size: var(--arcl-type-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arcl-command-menu__search {
    display: flex;
    align-items: center;
    gap: var(--arcl-space-3);
    padding: var(--arcl-space-3) var(--arcl-space-4);
    border-bottom: var(--arcl-border-width) solid var(--arcl-border);
    color: var(--arcl-text-muted);
  }
  .arcl-command-menu__search input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: none;
    color: var(--arcl-text);
    background: transparent;
    font: inherit;
    font-size: var(--arcl-type-base);
  }
  .arcl-command-menu__list {
    display: grid;
    max-height: min(24rem, 55vh);
    overflow-y: auto;
    padding: var(--arcl-space-1);
    outline: none;
  }
  .arcl-command-menu__group {
    padding: var(--arcl-space-2) var(--arcl-space-3) var(--arcl-space-1);
    color: var(--arcl-text-muted);
    font-size: var(--arcl-type-xs);
    font-weight: 600;
  }
  .arcl-command-menu__option {
    display: grid;
    grid-template-columns: 1rem minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--arcl-space-3);
    min-height: 2.25rem;
    padding: 0 var(--arcl-space-3);
    border-radius: var(--arcl-radius-sm);
    color: var(--arcl-text);
    font-size: var(--arcl-type-sm);
    cursor: pointer;
  }
  .arcl-command-menu__option :global(.arcl-icon) {
    color: var(--arcl-text-muted);
  }
  .arcl-command-menu__option[aria-selected="true"] {
    background: var(--arcl-surface-hover);
    box-shadow: inset 2px 0 0 var(--arcl-focus);
  }
  .arcl-command-menu__option[aria-selected="true"] :global(.arcl-icon) {
    color: var(--arcl-focus);
  }
  .arcl-command-menu__label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arcl-command-menu__label b {
    font-weight: 650;
  }
  .arcl-command-menu__detail {
    color: var(--arcl-text-muted);
  }
  .arcl-command-menu__keys {
    display: flex;
    gap: var(--arcl-space-1);
  }
  kbd {
    display: inline-flex;
    min-width: 1.125rem;
    height: 1.125rem;
    align-items: center;
    justify-content: center;
    padding: 0 0.25rem;
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: 4px;
    color: var(--arcl-text-muted);
    background: var(--arcl-surface);
    font: 500 var(--arcl-type-xs) / 1 var(--arcl-font-ui);
  }
  .arcl-command-menu__empty {
    margin: 0;
    padding: var(--arcl-space-4);
    color: var(--arcl-text-muted);
    font-size: var(--arcl-type-sm);
  }
  .arcl-command-menu__hints {
    display: flex;
    flex-wrap: wrap;
    gap: var(--arcl-space-4);
    padding: var(--arcl-space-2) var(--arcl-space-4);
    border-top: var(--arcl-border-width) solid var(--arcl-border);
    color: var(--arcl-text-muted);
    font-size: var(--arcl-type-xs);
  }
  .arcl-command-menu__hints span {
    display: inline-flex;
    align-items: center;
    gap: var(--arcl-space-1);
  }
  .arcl-command-menu__end {
    margin-inline-start: auto;
  }
</style>
