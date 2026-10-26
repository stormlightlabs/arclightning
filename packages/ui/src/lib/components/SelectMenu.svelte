<script module lang="ts">
  import type { SelectOption } from "./Select.svelte";

  /** Props for a compact select that opens a listbox. */
  export interface SelectMenuProps {
    label: string;
    options: SelectOption[];
    /** The chosen option's value. */
    value?: string;
    /** Called with the new value when the user chooses a different option. */
    onchange?: (value: string) => void;
    id?: string;
    disabled?: boolean;
    /** What the trigger shows when `value` names no option. */
    placeholder?: string;
  }
</script>

<script lang="ts">
  import Icon from "./Icon.svelte";
  import { TYPEAHEAD_TIMEOUT, listboxKey, typeahead } from "../listbox";

  const generatedId = $props.id();
  let {
    id = generatedId,
    label,
    options,
    value = $bindable(""),
    onchange,
    disabled = false,
    placeholder = "",
  }: SelectMenuProps = $props();

  let open = $state(false);
  let active = $state(0);
  let query = "";
  let queryTimer: ReturnType<typeof setTimeout> | undefined;
  let root: HTMLDivElement;
  let listbox = $state<HTMLDivElement>();
  /** Whether the listbox aligns to the trigger's right edge, to stay inside the viewport. */
  let alignEnd = $state(false);

  let selectedIndex = $derived(options.findIndex((option) => option.value === value));
  let labelId = $derived(`${id}-label`);
  let listId = $derived(`${id}-listbox`);
  const optionId = (index: number) => `${id}-option-${index}`;

  $effect(() => {
    const element = listbox;
    if (!element) return;
    const place = () => {
      const gutter = 8;
      const trigger = root.getBoundingClientRect();
      const width = element.offsetWidth;
      alignEnd =
        trigger.left + width > document.documentElement.clientWidth - gutter && trigger.right - width >= gutter;
    };
    place();
    // The listbox can change width while open, such as when its options change.
    const observer = new ResizeObserver(place);
    observer.observe(element);
    return () => observer.disconnect();
  });

  $effect(() => {
    if (open) listbox?.querySelector(`#${CSS.escape(optionId(active))}`)?.scrollIntoView({ block: "nearest" });
  });

  function show() {
    active = Math.max(selectedIndex, 0);
    open = true;
  }

  function hide() {
    open = false;
    query = "";
    clearTimeout(queryTimer);
  }

  function choose(index: number) {
    const option = options[index];
    if (!option || option.disabled) return;
    if (option.value !== value) {
      value = option.value;
      onchange?.(option.value);
    }
  }

  /** The index of the option `target` sits in, or `null` outside any option. */
  function optionAt(target: EventTarget | null): number | null {
    const option = (target as Element | null)?.closest?.<HTMLElement>("[role=option]");
    return option ? Number(option.dataset.index) : null;
  }

  function onkeydown(event: KeyboardEvent) {
    if (disabled) return;
    const from = open ? active : Math.max(selectedIndex, 0);
    const step = listboxKey(event, { open, active: from, searching: query !== "" }, options);
    if (step === null) return;
    if (step.prevent) event.preventDefault();
    let next = step.active;
    if (step.type !== undefined) {
      query += step.type;
      clearTimeout(queryTimer);
      queryTimer = setTimeout(() => (query = ""), TYPEAHEAD_TIMEOUT);
      next = typeahead(options, from, query) ?? from;
    }
    if (step.commit) choose(next);
    if (step.open) {
      if (!open) open = true;
      active = next;
    } else {
      hide();
    }
  }
</script>

<svelte:window
  onpointerdown={(event) => {
    if (open && !root.contains(event.target as Node)) hide();
  }}
/>

<div class="arcl-select-menu" bind:this={root}>
  <div
    {id}
    class="arcl-select-menu__trigger"
    role="combobox"
    tabindex={disabled ? -1 : 0}
    aria-labelledby={labelId}
    aria-controls={listId}
    aria-expanded={open}
    aria-haspopup="listbox"
    aria-activedescendant={open && options[active] ? optionId(active) : undefined}
    aria-disabled={disabled || undefined}
    onclick={() => {
      if (!disabled) open ? hide() : show();
    }}
    {onkeydown}
    onfocusout={(event) => {
      if (open && !root.contains(event.relatedTarget as Node)) hide();
    }}
  >
    <span class="arcl-select-menu__label" id={labelId}>{label}</span>
    <span class="arcl-select-menu__value">{options[selectedIndex]?.label ?? placeholder}</span>
    <Icon name="chevronDown" size="1rem" />
  </div>
  {#if open}
    <!-- Focus stays on the combobox: pressing an option must not move it, and
         the listbox answers keys only if something else focuses it. -->
    <div
      class="arcl-select-menu__listbox"
      role="listbox"
      id={listId}
      aria-labelledby={labelId}
      tabindex="-1"
      bind:this={listbox}
      data-align={alignEnd ? "end" : undefined}
      onmousedown={(event) => event.preventDefault()}
      {onkeydown}
      onpointermove={(event) => {
        const index = optionAt(event.target);
        if (index !== null && !options[index].disabled) active = index;
      }}
      onclick={(event) => {
        const index = optionAt(event.target);
        if (index === null || options[index].disabled) return;
        choose(index);
        hide();
      }}
    >
      {#each options as option, index (option.value)}
        <div
          class="arcl-select-menu__option"
          role="option"
          id={optionId(index)}
          tabindex="-1"
          aria-selected={index === selectedIndex}
          aria-disabled={option.disabled || undefined}
          data-active={index === active || undefined}
          data-index={index}
        >
          <span>{option.label}</span>
          {#if index === selectedIndex}<Icon name="check" size="1rem" />{/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .arcl-select-menu {
    position: relative;
    display: inline-block;
    font-family: var(--arcl-font-ui);
    font-size: var(--arcl-type-sm);
  }
  .arcl-select-menu__trigger {
    display: inline-flex;
    min-height: 2rem;
    align-items: center;
    gap: var(--arcl-space-2);
    padding: 0 var(--arcl-space-2) 0 var(--arcl-space-3);
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-control);
    color: var(--arcl-text);
    background: var(--arcl-surface);
    box-shadow: var(--arcl-shadow-control);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  .arcl-select-menu__trigger:hover {
    background: var(--arcl-surface-hover);
  }
  .arcl-select-menu__trigger[aria-disabled="true"] {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .arcl-select-menu__label {
    color: var(--arcl-text-muted);
  }
  .arcl-select-menu__value {
    max-width: 16rem;
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
  }
  .arcl-select-menu__trigger :global(.arcl-icon) {
    color: var(--arcl-text-muted);
  }
  .arcl-select-menu__listbox {
    position: absolute;
    z-index: 20;
    top: calc(100% + var(--arcl-space-1));
    left: 0;
    display: grid;
    min-width: max(100%, 12rem);
    max-width: min(24rem, calc(100vw - 2rem));
    max-height: 18rem;
    overflow-y: auto;
    padding: var(--arcl-space-1);
    border: var(--arcl-border-width) solid var(--arcl-border);
    border-radius: var(--arcl-radius-md);
    background: var(--arcl-surface);
    box-shadow: var(--arcl-shadow-overlay);
    animation: arcl-select-menu-in var(--arcl-duration-fast) var(--arcl-ease-out);
  }
  .arcl-select-menu__listbox[data-align="end"] {
    right: 0;
    left: auto;
  }
  .arcl-select-menu__option {
    display: flex;
    min-height: 2rem;
    align-items: center;
    justify-content: space-between;
    gap: var(--arcl-space-3);
    padding-inline: var(--arcl-space-2);
    border-radius: var(--arcl-radius-sm);
    color: var(--arcl-text);
    cursor: pointer;
  }
  .arcl-select-menu__option > span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .arcl-select-menu__option[data-active] {
    background: var(--arcl-surface-hover);
    box-shadow: inset 2px 0 0 var(--arcl-focus);
  }
  .arcl-select-menu__option[aria-selected="true"] {
    font-weight: 600;
  }
  .arcl-select-menu__option[aria-selected="true"] :global(.arcl-icon) {
    color: var(--arcl-focus);
  }
  .arcl-select-menu__option[aria-disabled="true"] {
    opacity: 0.55;
    cursor: not-allowed;
  }
  @keyframes arcl-select-menu-in {
    from {
      opacity: 0;
      transform: translateY(-0.25rem);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .arcl-select-menu__listbox {
      animation: none;
    }
  }
</style>
