<script lang="ts">
	import type { DocHeading } from '../table-of-contents';
	import TableOfContents from '../components/TableOfContents.svelte';

	let { headings }: { headings: DocHeading[] } = $props();
</script>

<TableOfContents {headings} />
{#each headings as heading}
	<h2 id={heading.slug} style="margin-bottom: 150vh">{heading.title}</h2>
{/each}
