<script lang="ts">
	import type { Snippet } from 'svelte';
	import { docs, getAdjacentDocs, type Doc } from '$lib/content';
	import { absoluteUrl, site } from '$lib/site';
	import CopyCode from './CopyCode.svelte';
	import CopyMarkdown from './CopyMarkdown.svelte';
	import PageNavigation from './PageNavigation.svelte';
	import Sidebar from './Sidebar.svelte';
	import SiteFooter from './SiteFooter.svelte';
	import SiteHeader from './SiteHeader.svelte';
	import TableOfContents from './TableOfContents.svelte';

	let { doc, content }: { doc: Doc; content: Snippet } = $props();
	const adjacent = $derived(getAdjacentDocs(doc.slug));
	const url = $derived(absoluteUrl(`/${doc.slug}/`));
</script>

<svelte:head>
	<title>{doc.title} · Arc Lightning docs</title>
	<meta name="description" content={doc.description} />
	<meta property="og:title" content={`${doc.title} · Arc Lightning docs`} />
	<meta property="og:description" content={doc.description} />
	<meta property="og:type" content="article" />
	<meta property="og:url" content={url} />
	<meta property="og:site_name" content={site.name} />
	<meta property="og:image" content={site.image.url} />
	<meta property="og:image:type" content="image/png" />
	<meta property="og:image:width" content={String(site.image.width)} />
	<meta property="og:image:height" content={String(site.image.height)} />
	<meta property="og:image:alt" content={site.image.alt} />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:image" content={site.image.url} />
	<meta name="twitter:image:alt" content={site.image.alt} />
	<link rel="canonical" href={url} />
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
