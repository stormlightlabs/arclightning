<script module lang="ts">
  /** Props for a CodeMirror 6 Markdown editor, read-only unless told otherwise. */
  export interface MarkdownEditorProps {
    value: string;
    /** The editor's accessible name. */
    label: string;
    readonly?: boolean;
    /** Called with the whole text after each edit. */
    onchange?: (value: string) => void;
  }
</script>

<script lang="ts">
  import { markdown } from "@codemirror/lang-markdown";
  import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
  import { Compartment, EditorState } from "@codemirror/state";
  import { EditorView, keymap } from "@codemirror/view";
  import { tags } from "@lezer/highlight";
  import { onMount } from "svelte";

  let { value, label, readonly = true, onchange }: MarkdownEditorProps = $props();
  let host: HTMLDivElement;
  let view = $state<EditorView>();
  const access = new Compartment();

  // Colours come from the theme tokens, so the editor follows light and dark.
  const highlight = HighlightStyle.define([
    { tag: tags.heading, color: "var(--arcl-heading)", fontWeight: "650" },
    { tag: tags.strong, fontWeight: "650" },
    { tag: tags.emphasis, fontStyle: "italic" },
    { tag: [tags.link, tags.url], color: "var(--arcl-link)" },
    { tag: tags.monospace, color: "var(--arcl-warning)" },
    { tag: [tags.quote, tags.processingInstruction, tags.meta], color: "var(--arcl-text-muted)" },
  ]);

  function accessFor(readonly: boolean) {
    return [
      EditorState.readOnly.of(readonly),
      EditorView.contentAttributes.of({ "aria-label": label, "aria-readonly": String(readonly) }),
    ];
  }

  onMount(() => {
    view = new EditorView({
      parent: host,
      state: EditorState.create({
        doc: value,
        extensions: [
          markdown(),
          syntaxHighlighting(highlight),
          EditorView.lineWrapping,
          access.of(accessFor(readonly)),
          // Escape leaves the editor, so the next Escape reaches the page.
          keymap.of([{ key: "Escape", run: (editor) => (editor.contentDOM.blur(), true) }]),
          EditorView.updateListener.of((update) => {
            if (update.docChanged) onchange?.(update.state.doc.toString());
          }),
        ],
      }),
    });
    return () => view?.destroy();
  });

  $effect(() => {
    if (view && view.state.doc.toString() !== value) {
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } });
    }
  });

  $effect(() => {
    view?.dispatch({ effects: access.reconfigure(accessFor(readonly)) });
  });
</script>

<div class="arcl-markdown-editor" bind:this={host}></div>

<style>
  .arcl-markdown-editor { overflow: hidden; border: var(--arcl-border-width) solid var(--arcl-border); border-radius: var(--arcl-radius-md); background: var(--arcl-canvas); }
  .arcl-markdown-editor :global(.cm-editor) { color: var(--arcl-text); font: var(--arcl-type-sm) / 1.6 var(--arcl-font-code); }
  .arcl-markdown-editor :global(.cm-editor.cm-focused) { outline: var(--arcl-focus-width) solid var(--arcl-focus); outline-offset: -2px; }
  .arcl-markdown-editor :global(.cm-scroller) { font-family: inherit; }
  .arcl-markdown-editor :global(.cm-content) { padding: var(--arcl-space-3) var(--arcl-space-4); caret-color: var(--arcl-text); }
  .arcl-markdown-editor :global(.cm-selectionBackground), .arcl-markdown-editor :global(.cm-focused .cm-selectionBackground) { background: var(--arcl-info-surface); }
</style>
