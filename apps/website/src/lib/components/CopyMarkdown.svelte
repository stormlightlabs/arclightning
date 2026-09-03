<script lang="ts">
	import { Icon } from '@arclightning/ui';
	let { markdown, slug }: { markdown: string; slug: string } = $props();
	let label = $state('Copy Markdown');

	async function copy(event: MouseEvent): Promise<void> {
		event.preventDefault();
		try {
			await navigator.clipboard.writeText(markdown);
			label = 'Copied';
			window.setTimeout(() => (label = 'Copy Markdown'), 1600);
		} catch {
			window.location.assign((event.currentTarget as HTMLAnchorElement).href);
		}
	}
</script>

<a class="copy-markdown" href={`/${slug}.md`} onclick={copy} aria-live="polite"
	><Icon name="markdown" size={17} /><span>{label}</span></a>
