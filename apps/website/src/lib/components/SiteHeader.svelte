<script lang="ts">
	import { Icon } from '@arclightning/ui';
	import { afterNavigate } from '$app/navigation';
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
	let menu = $state<HTMLDetailsElement>();

	// A docs page keeps this header across navigation, so close the menu after a link in it is followed.
	afterNavigate(() => {
		if (menu) menu.open = false;
	});

	/**
	 * Close the open menu on Escape and return focus to its toggle. Escape counts from inside the menu or with nothing
	 * focused (the target is the body), so other controls keep their own Escape. The search dialog handles its own Escape.
	 */
	function closeOnEscape(event: KeyboardEvent): void {
		if (event.key !== 'Escape' || !menu?.open || event.defaultPrevented) return;
		const fromPage = event.target === document.body || event.target === document.documentElement;
		if (!fromPage && !(event.target instanceof Node && menu.contains(event.target))) return;
		if (event.target instanceof Element && event.target.closest('dialog')) return;
		menu.open = false;
		menu.querySelector('summary')?.focus();
	}
</script>

<svelte:document onkeydown={closeOnEscape} />

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
			<details class="mobile-menu" bind:this={menu}>
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
