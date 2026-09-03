<script lang="ts">
	import { Icon } from '@arclightning/ui';
	let { id = 'docs-search' }: { id?: string } = $props();
	const dialogId = $derived(`${id}-dialog`);
</script>

<div class="search" data-doc-search>
	<button
		class="search-trigger"
		type="button"
		data-search-trigger
		aria-controls={dialogId}
		aria-expanded="false"
		aria-haspopup="dialog"
		aria-label="Open documentation search (Ctrl+K or Cmd+K)">
		<Icon name="search" size={17} /><span class="search-trigger__label">Search docs</span><kbd aria-hidden="true"
			>⌘ K</kbd>
	</button>
	<a class="search-fallback" href="/search/">Browse documentation</a>
	<dialog
		class="search-dialog"
		id={dialogId}
		data-search-dialog
		aria-labelledby={`${id}-title`}
		aria-describedby={`${id}-description`}>
		<div class="search-dialog__inner">
			<header>
				<div>
					<p class="eyebrow">Documentation search</p>
					<h2 id={`${id}-title`}>Search the docs</h2>
				</div>
				<button type="button" data-search-close aria-label="Close search"><Icon name="close" size={19} /></button>
			</header>
			<p id={`${id}-description`}>
				Find workflows, commands, and planning concepts across the Arc Lightning documentation.
			</p>
			<form role="search" action="/search/" method="get" data-search-form>
				<label class="sr-only" for={`${id}-input`}>Search documentation</label>
				<div class="search-input">
					<Icon name="search" size={17} /><input
						id={`${id}-input`}
						name="q"
						type="search"
						data-search-input
						placeholder="Search pages, commands, and concepts…"
						autocomplete="off"
						role="combobox"
						aria-autocomplete="list"
						aria-expanded="false"
						aria-controls={`${id}-results`} />
				</div>
			</form>
			<p class="sr-only" data-search-status role="status" aria-live="polite"></p>
			<div class="search-results" id={`${id}-results`} data-search-results>
				<p class="search-message">Start typing to search the documentation.</p>
			</div>
			<footer>
				<span><kbd>↑</kbd><kbd>↓</kbd> navigate</span><span><kbd>Enter</kbd> open</span><span
					><kbd>Esc</kbd> close</span>
			</footer>
		</div>
	</dialog>
</div>
