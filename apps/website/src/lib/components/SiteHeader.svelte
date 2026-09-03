<script lang="ts">
	import { Icon } from '@arclightning/ui';
	import type { Doc } from '$lib/content';
	import Search from './Search.svelte';
	import ThemeToggle from './ThemeToggle.svelte';

	let { docs, currentSlug = '' }: { docs: Doc[]; currentSlug?: string } = $props();
	const links = [
		{ label: 'Start', href: '/quick-start/', slug: 'quick-start' },
		{ label: 'Guides', href: '/guides/tasks/', slug: 'guides' },
		{ label: 'Reference', href: '/reference/manual/', slug: 'reference' }
	];
	const isCurrent = (slug: string): boolean => currentSlug === slug || currentSlug.startsWith(`${slug}/`);
</script>

<a class="skip-link" href="#main-content">Skip to content</a>
<header class="site-header" data-pagefind-ignore>
	<div class="site-header__inner">
		<a class="brand" href="/" aria-label="Arc Lightning home"
			><img class="brand__mark" src="/favicon.svg" alt="" /><span>Arc Lightning</span></a>
		<nav class="primary-nav" aria-label="Primary navigation">
			{#each links as link}<a class:active={isCurrent(link.slug)} href={link.href}>{link.label}</a>{/each}
			<a class="github-link" href="https://github.com/stormlightlabs/arclightning"
				><Icon name="github" size={17} /> GitHub</a>
		</nav>
		<div class="site-header__actions">
			<div class="desktop-search"><Search id="header-search" /></div>
			<ThemeToggle />
			<details class="mobile-menu">
				<summary aria-label="Open documentation menu"><Icon name="menu" size={18} /><span>Menu</span></summary>
				<div class="mobile-menu__panel">
					<Search id="mobile-search" />
					<nav aria-label="Mobile navigation">
						{#each links as link}<a class:active={isCurrent(link.slug)} href={link.href}>{link.label}</a>{/each}
						<a class="github-link" href="https://github.com/stormlightlabs/arclightning"
							><Icon name="github" size={17} /> GitHub</a>
					</nav>
					<div class="mobile-doc-links">
						{#each docs as doc}<a class:active={doc.slug === currentSlug} href={`/${doc.slug}/`}>{doc.title}</a>{/each}
					</div>
				</div>
			</details>
		</div>
	</div>
</header>
