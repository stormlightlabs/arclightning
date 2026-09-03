<script module lang="ts">
  /** Props for safe Markdown presentation. Raw HTML is escaped. */
  export interface MarkdownProps { source: string; label?: string; }
</script>

<script lang="ts">
  import { micromark } from "micromark";
  let { source, label = "Markdown content" }: MarkdownProps = $props();
  let rendered = $derived(micromark(source));
</script>

<div class="arcl-markdown" aria-label={label}>{@html rendered}</div>

<style>
  .arcl-markdown { max-width: 72ch; color: var(--arcl-text); line-height: 1.7; overflow-wrap: anywhere; }
  .arcl-markdown :global(> :first-child) { margin-top: 0; }
  .arcl-markdown :global(> :last-child) { margin-bottom: 0; }
  .arcl-markdown :global(h1), .arcl-markdown :global(h2), .arcl-markdown :global(h3) { margin-block: 1.5em 0.5em; color: var(--arcl-heading); font-family: var(--arcl-font-ui); line-height: 1.2; text-wrap: balance; }
  .arcl-markdown :global(p), .arcl-markdown :global(li) { text-wrap: pretty; }
  .arcl-markdown :global(a) { color: var(--arcl-link); text-underline-offset: 0.15em; }
  .arcl-markdown :global(code) { padding: 0.1em 0.3em; border-radius: var(--arcl-radius-sm); color: var(--arcl-warning); background: var(--arcl-surface-subtle); font-family: var(--arcl-font-code); }
  .arcl-markdown :global(pre) { overflow-x: auto; padding: var(--arcl-space-4); border: 1px solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-canvas); }
  .arcl-markdown :global(pre code) { padding: 0; color: var(--arcl-text); background: transparent; }
  .arcl-markdown :global(blockquote) { margin-inline: 0; padding-inline-start: var(--arcl-space-4); border-inline-start: 4px solid var(--arcl-accent); color: var(--arcl-text-muted); }
</style>
