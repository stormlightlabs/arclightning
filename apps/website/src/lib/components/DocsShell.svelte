<script lang="ts">
	import type { Snippet } from 'svelte';
	import { docs, getAdjacentDocs, type Doc } from '$lib/content';
	import CopyCode from './CopyCode.svelte';
	import CopyMarkdown from './CopyMarkdown.svelte';
	import PageNavigation from './PageNavigation.svelte';
	import Sidebar from './Sidebar.svelte';
	import SiteFooter from './SiteFooter.svelte';
	import SiteHeader from './SiteHeader.svelte';
	import TableOfContents from './TableOfContents.svelte';

	let { doc, content }: { doc: Doc; content: Snippet } = $props();
	const adjacent = $derived(getAdjacentDocs(doc.slug));
</script>

<svelte:head>
	<title>{doc.title} · Arc Lightning docs</title>
	<meta name="description" content={doc.description} />
	<meta property="og:title" content={`${doc.title} · Arc Lightning docs`} />
	<meta property="og:description" content={doc.description} />
	<meta property="og:type" content="article" />
	<meta property="og:image" content="https://arclightning.stormlightlabs.org/social-card.svg" />
	<meta name="twitter:card" content="summary_large_image" />
	<link rel="canonical" href={`https://arclightning.stormlightlabs.org/${doc.slug}/`} />
</svelte:head>

<SiteHeader {docs} currentSlug={doc.slug} />
<div class="docs-layout">
	<aside class="sidebar" aria-label="Documentation navigation"><Sidebar {docs} currentSlug={doc.slug} /></aside>
	<main id="main-content" class="docs-main">
		<nav class="breadcrumbs" aria-label="Breadcrumbs" data-pagefind-ignore>
			<a href="/">Home</a><span aria-hidden="true">/</span><span>{doc.section}</span><span aria-hidden="true">/</span
			><span aria-current="page">{doc.title}</span>
		</nav>
		<article data-pagefind-body>
			<header class="doc-heading">
				<p class="eyebrow">{doc.section}</p>
				<h1>{doc.title}</h1>
				<div class="doc-heading__secondary">
					<CopyMarkdown markdown={doc.markdown} slug={doc.slug} />
					<p>{doc.description}</p>
				</div>
			</header>
			<div class="doc-content">{@render content()}</div>
			{#key doc.slug}<CopyCode />{/key}
		</article>
		<PageNavigation previous={adjacent.previous} next={adjacent.next} />
	</main>
	<aside class="toc-column" aria-label="Table of contents"><TableOfContents headings={doc.toc} /></aside>
</div>
<SiteFooter />
