<script lang="ts">
	import { Icon } from '@arclightning/ui';
	import type { Doc } from '$lib/content';
	let { previous, next }: { previous?: Doc; next?: Doc } = $props();
</script>

<nav class="page-navigation" aria-label="Page navigation" data-pagefind-ignore>
	{#if previous}<a href={`/${previous.slug}/`}
			><span>Previous</span><strong><Icon name="arrowLeft" size={16} />{previous.title}</strong></a
		>{:else}<span></span>{/if}
	{#if next}<a class="next" href={`/${next.slug}/`}
			><span>Next</span><strong>{next.title}<Icon name="arrowRight" size={16} /></strong></a
		>{/if}
</nav>
