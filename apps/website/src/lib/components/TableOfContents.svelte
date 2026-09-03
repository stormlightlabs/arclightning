<script lang="ts">
	import type { DocHeading } from '$lib/table-of-contents';
	let { headings }: { headings: DocHeading[] } = $props();
	let active = $state<string | null>(null);

	$effect(() => {
		const elements = headings
			.map((heading) => document.getElementById(heading.slug))
			.filter((element): element is HTMLElement => element instanceof HTMLElement);
		if (!elements.length) return;
		const observer = new IntersectionObserver(
			(entries) => {
				const visible = entries.find((entry) => entry.isIntersecting);
				if (visible) active = visible.target.id;
			},
			{ rootMargin: '-20% 0px -70%', threshold: 0 }
		);
		elements.forEach((element) => observer.observe(element));
		return () => observer.disconnect();
	});
</script>

{#if headings.length}
	<nav class="toc" aria-label="On this page" data-pagefind-ignore>
		<p>On this page</p>
		<ul>
			{#each headings as heading}<li class:toc__subitem={heading.level === 3} class:active={heading.slug === active}>
					<a href={`#${heading.slug}`} aria-current={heading.slug === active ? 'location' : undefined}
						>{heading.title}</a>
				</li>{/each}
		</ul>
	</nav>
{/if}
