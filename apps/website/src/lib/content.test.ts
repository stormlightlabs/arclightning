import { describe, expect, it } from 'vitest';
import { GET as markdown, entries as markdownEntries } from '../routes/[...slug].md/+server';
import { GET as llms } from '../routes/llms.txt/+server';
import { GET as sitemap } from '../routes/sitemap.xml/+server';
import {
	type DocSection,
	docSections,
	docs,
	getAdjacentDocs,
	getDoc,
	getRedirect,
	redirectPage,
	redirects
} from './content';

describe('docs', () => {
	it('gives every page a title, a description, a section and its Markdown', () => {
		expect(docs.length).toBeGreaterThan(0);
		for (const doc of docs) {
			expect(doc.title, doc.slug).toBeTruthy();
			expect(doc.description, doc.slug).toBeTruthy();
			expect(docSections).toContain(doc.section);
			expect(doc.markdown, doc.slug).toContain(doc.title);
		}
	});

	it('files each page under the section its URL prefix names, in Diátaxis order', () => {
		const prefixes: Record<DocSection, string> = {
			Tutorials: 'tutorials/',
			'How-to guides': 'how-to/',
			Explanation: 'explanation/',
			Reference: 'reference/'
		};
		for (const doc of docs) expect(doc.slug.startsWith(prefixes[doc.section]), doc.slug).toBe(true);
		const order = docs.map((doc) => docSections.indexOf(doc.section));
		expect(order).toEqual([...order].sort((left, right) => left - right));
		for (const section of docSections)
			expect(
				docs.some((doc) => doc.section === section),
				section
			).toBe(true);
	});

	it('orders pages for the sidebar and links neighbours for previous and next', () => {
		expect(docs.slice(0, 2).map((doc) => doc.slug)).toEqual(['tutorials/quick-start', 'tutorials/agent-feature']);
		expect(getAdjacentDocs('tutorials/quick-start')).toEqual({
			previous: undefined,
			next: getDoc('tutorials/agent-feature')
		});
		const last = docs[docs.length - 1];
		expect(getAdjacentDocs(last.slug)).toEqual({ previous: docs[docs.length - 2], next: undefined });
		expect(getAdjacentDocs('missing')).toEqual({ previous: undefined, next: undefined });
	});
});

describe('redirects', () => {
	it('points each retired slug at a page that exists, and retires no current page', () => {
		expect(Object.keys(redirects).length).toBeGreaterThan(0);
		for (const [from, to] of Object.entries(redirects)) {
			expect(getDoc(from), from).toBeUndefined();
			expect(getDoc(to), to).toBeDefined();
			expect(getRedirect(from)).toBe(to);
		}
	});

	it('serves the replacement page as Markdown at each retired .md URL', async () => {
		const slugs = (await markdownEntries()).map((entry) => entry.slug);
		for (const [from, to] of Object.entries(redirects)) {
			expect(slugs, from).toContain(from);
			const response = await markdown({ params: { slug: from } } as Parameters<typeof markdown>[0]);
			expect(response.headers.get('content-type')).toBe('text/markdown; charset=utf-8');
			expect(await response.text()).toBe(getDoc(to)?.markdown);
		}
	});

	it('redirects every retired guide, the old quick start and the old overview to a current page', () => {
		const retired = [
			'overview',
			'quick-start',
			...[
				'agents',
				'api',
				'artifact',
				'automation',
				'captures',
				'compaction',
				'epics',
				'ideas',
				'inspect-work',
				'lifecycle',
				'local-development',
				'mcp',
				'milestones',
				'notes',
				'phases',
				'plans',
				'releases',
				'rest',
				'specs',
				'tasks',
				'web-app',
				'workspace'
			].map((name) => `guides/${name}`)
		];
		for (const from of retired) {
			const to = getRedirect(from);
			expect(to, from).toBeDefined();
			expect(getDoc(to ?? ''), `${from} -> ${to}`).toBeDefined();
			expect(getRedirect(to ?? ''), `${from} -> ${to} chains`).toBeUndefined();
		}
		expect(docs.filter((doc) => doc.slug.startsWith('guides/'))).toEqual([]);
	});

	it('carries the URL fragment in the script and keeps a meta refresh without it', () => {
		const html = redirectPage('how-to/capture-and-triage');
		expect(html).toContain('<script>location.replace("/how-to/capture-and-triage/" + location.hash);</script>');
		expect(html).toContain('<meta http-equiv="refresh" content="0;url=/how-to/capture-and-triage/">');
		expect(html.indexOf('<script>')).toBeLessThan(html.indexOf('http-equiv="refresh"'));
	});

	it('ignores slugs that only name inherited object properties', () => {
		expect(getRedirect('constructor')).toBeUndefined();
		expect(getRedirect('how-to/work-a-task')).toBeUndefined();
	});
});

describe('generated files', () => {
	it('lists the landing page and every docs page in the sitemap', async () => {
		const body = await sitemap().text();
		expect(body).toContain('<loc>https://arcl.stormlightlabs.org/</loc>');
		for (const doc of docs) expect(body).toContain(`/${doc.slug}/</loc>`);
	});

	it('links every page as Markdown in llms.txt', async () => {
		const body = await llms().text();
		expect(body.startsWith('# Arc Lightning\n')).toBe(true);
		for (const doc of docs) expect(body).toContain(`- [${doc.title}](/${doc.slug}.md): ${doc.description}`);
	});
});
