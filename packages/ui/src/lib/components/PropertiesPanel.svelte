<script module lang="ts">
  import type { Priority } from "./PriorityBadge.svelte";
  import type { RecordStatus } from "./StatusBadge.svelte";

  /** One labelled value in a properties panel. `null` reads as "None". */
  export interface PropertyItem {
    label: string;
    value: string | null;
    /** The record the value names; the value links to it. */
    href?: string;
    /** Show the value as a status badge. */
    status?: RecordStatus;
    /** Show the value as a priority badge. */
    priority?: Priority;
  }

  /** Props for a record's collapsible properties panel. */
  export interface PropertiesPanelProps {
    properties: PropertyItem[];
    /** Whether the values show; bind to it to toggle from a shortcut. */
    open?: boolean;
    label?: string;
    /** The key that toggles the panel, shown on its button. */
    shortcut?: string;
    ontoggle?: (open: boolean) => void;
  }
</script>

<script lang="ts">
  import PriorityBadge from "./PriorityBadge.svelte";
  import StatusBadge from "./StatusBadge.svelte";

  let { properties, open = $bindable(true), label = "Properties", shortcut, ontoggle }: PropertiesPanelProps = $props();
  const listId = $props.id();

  function toggle() {
    open = !open;
    ontoggle?.(open);
  }
</script>

<aside class="arcl-properties" aria-label={label} data-open={open}>
  <h2>
    <button type="button" aria-expanded={open} aria-controls={listId} aria-keyshortcuts={shortcut} onclick={toggle}>
      <span class="arcl-properties__chevron" aria-hidden="true"></span>
      {label}
      {#if shortcut}<kbd>{shortcut}</kbd>{/if}
    </button>
  </h2>
  <dl id={listId} hidden={!open}>
    {#each properties as property (property.label)}
      <div>
        <dt>{property.label}</dt>
        <dd>
          {#if property.value === null}
            <span class="arcl-properties__none">None</span>
          {:else if property.status}
            <StatusBadge status={property.status} />
          {:else if property.priority}
            <PriorityBadge priority={property.priority} />
          {:else if property.href}
            <a href={property.href}>{property.value}</a>
          {:else}
            {property.value}
          {/if}
        </dd>
      </div>
    {/each}
  </dl>
</aside>

<style>
  .arcl-properties { border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-surface); font-size: var(--arcl-type-sm); }
  h2 { margin: 0; font: inherit; }
  button { display: flex; align-items: center; gap: var(--arcl-space-2); width: 100%; padding: var(--arcl-space-2) var(--arcl-space-3); border: 0; border-radius: inherit; color: var(--arcl-heading); background: transparent; font: 600 var(--arcl-type-sm) / 1.4 var(--arcl-font-ui); text-align: start; cursor: pointer; }
  button:hover { background: var(--arcl-surface-hover); }
  .arcl-properties__chevron { width: 0.4rem; height: 0.4rem; border-right: 1.5px solid currentColor; border-bottom: 1.5px solid currentColor; transform: rotate(-45deg); transition: transform var(--arcl-duration-fast) var(--arcl-ease-out); }
  [data-open="true"] .arcl-properties__chevron { transform: rotate(45deg); }
  kbd { margin-inline-start: auto; padding: 0 0.3rem; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-sm); color: var(--arcl-text-muted); font: 500 var(--arcl-type-xs) / 1.4 var(--arcl-font-code); }
  dl { display: grid; margin: 0; padding: var(--arcl-space-1) var(--arcl-space-3) var(--arcl-space-3); border-top: var(--arcl-border-width) solid var(--arcl-surface-subtle); }
  dl[hidden] { display: none; }
  dl > div { display: grid; grid-template-columns: 6rem minmax(0, 1fr); align-items: center; gap: var(--arcl-space-2); min-height: 2rem; }
  dt { color: var(--arcl-text-muted); }
  dd { margin: 0; overflow-wrap: anywhere; color: var(--arcl-text); }
  a { color: var(--arcl-link); text-decoration: none; }
  a:hover { text-decoration: underline; }
  .arcl-properties__none { color: var(--arcl-text-muted); }
  @media (prefers-reduced-motion: reduce) { .arcl-properties__chevron { transition: none; } }
</style>
