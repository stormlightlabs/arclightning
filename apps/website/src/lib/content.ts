import type { Component } from 'svelte';
import type { DocHeading } from './table-of-contents';
import { redirects } from './redirects';

export { redirects };

/** A documentation navigation group, one per Diátaxis type. */
export type DocSection = 'Tutorials' | 'How-to guides' | 'Explanation' | 'Reference';

/** Front matter generated for a documentation page. */
export type DocMetadata = {
	title: string;
	description: string;
	toc?: DocHeading[];
};

/** One Markdown documentation page and its stable URL slug. */
export type Doc = DocMetadata & {
	slug: string;
	section: DocSection;
	component: Component;
	markdown: string;
	toc: DocHeading[];
};

type MarkdownModule = { default: Component; metadata: DocMetadata };
const modules = import.meta.glob<MarkdownModule>('/src/content/docs/**/*.md', { eager: true });
const sources = import.meta.glob<string>('/src/content/docs/**/*.md', {
	eager: true,
	query: '?raw',
	import: 'default'
});

const navigationOrder = [
	'tutorials/quick-start',
	'tutorials/agent-feature',
	'how-to/install',
	'how-to/capture-and-triage',
	'how-to/write-a-spec',
	'how-to/plan-from-a-file',
	'how-to/work-a-task',
	'how-to/hand-off-and-resume',
	'how-to/keep-notes',
	'how-to/find-work',
	'how-to/cut-a-release',
	'how-to/compact-old-work',
	'how-to/share-the-workspace',
	'how-to/recover-the-workspace',
	'how-to/set-up-an-agent',
	'how-to/connect-mcp',
	'how-to/script-arcl',
	'how-to/query-graphql',
	'how-to/serve-the-api',
	'how-to/use-the-web-app',
	'how-to/publish-the-artifact',
	'how-to/develop-locally',
	'explanation/overview',
	'explanation/record-model',
	'explanation/readiness',
	'explanation/workspace',
	'explanation/claims-and-handoffs',
	'explanation/compaction',
	'reference/manual',
	'reference/deps',
	'reference/output',
	'reference/plan-file',
	'reference/workspace-format',
	'reference/mcp',
	'reference/graphql',
	'reference/rest',
	'reference/web-app',
	'reference/artifact'
] as const;

/** Documentation sections in display order. */
export const docSections: DocSection[] = ['Tutorials', 'How-to guides', 'Explanation', 'Reference'];

/** The section a page belongs to, from the first part of its slug. */
function sectionFor(slug: string): DocSection {
	if (slug.startsWith('tutorials/')) return 'Tutorials';
	if (slug.startsWith('how-to/')) return 'How-to guides';
	if (slug.startsWith('reference/')) return 'Reference';
	return 'Explanation';
}

function sourceToSlug(source: string): string {
	return source.replace('/src/content/docs/', '').replace(/\.md$/, '');
}

export const docs: Doc[] = Object.entries(modules)
	.map(([source, module]) => {
		const slug = sourceToSlug(source);
		return {
			...module.metadata,
			slug,
			section: sectionFor(slug),
			component: module.default,
			markdown: sources[source],
			toc: module.metadata.toc ?? []
		};
	})
	.sort((left, right) => {
		const leftIndex = navigationOrder.indexOf(left.slug as (typeof navigationOrder)[number]);
		const rightIndex = navigationOrder.indexOf(right.slug as (typeof navigationOrder)[number]);
		return (
			(leftIndex < 0 ? Number.MAX_SAFE_INTEGER : leftIndex) - (rightIndex < 0 ? Number.MAX_SAFE_INTEGER : rightIndex)
		);
	});

/** Find a documentation page by its route slug. */
export function getDoc(slug: string): Doc | undefined {
	return docs.find((doc) => doc.slug === slug);
}

/** Find the slug of the page that replaces a retired slug. */
export function getRedirect(slug: string): string | undefined {
	return Object.hasOwn(redirects, slug) ? redirects[slug] : undefined;
}

/**
 * The HTML a retired URL serves. The script carries the URL fragment to the
 * replacement page, so an old `#anchor` link still lands on its section; the
 * meta refresh covers readers without JavaScript and drops the fragment.
 */
export function redirectPage(target: string): string {
	const url = `/${target}/`;
	return [
		'<!doctype html>',
		'<meta charset="utf-8">',
		'<meta name="robots" content="noindex">',
		`<link rel="canonical" href="${url}">`,
		`<script>location.replace(${JSON.stringify(url)} + location.hash);</script>`,
		`<meta http-equiv="refresh" content="0;url=${url}">`,
		`<title>Moved to ${url}</title>`
	].join('\n');
}

/** Find the pages immediately before and after a documentation page. */
export function getAdjacentDocs(slug: string): { previous?: Doc; next?: Doc } {
	const index = docs.findIndex((doc) => doc.slug === slug);
	return {
		previous: index > 0 ? docs[index - 1] : undefined,
		next: index >= 0 && index < docs.length - 1 ? docs[index + 1] : undefined
	};
}
