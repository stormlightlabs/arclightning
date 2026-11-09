import { describe, expect, it } from 'vitest';
import { GET as llms } from '../routes/llms.txt/+server';
import { GET as sitemap } from '../routes/sitemap.xml/+server';
import { docSections, docs, getAdjacentDocs, getDoc, getRedirect, redirects } from './content';

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

	it('orders pages for the sidebar and links neighbours for previous and next', () => {
		expect(docs.slice(0, 2).map((doc) => doc.slug)).toEqual(['overview', 'quick-start']);
		expect(getAdjacentDocs('overview')).toEqual({ previous: undefined, next: getDoc('quick-start') });
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

	it('ignores slugs that only name inherited object properties', () => {
		expect(getRedirect('constructor')).toBeUndefined();
		expect(getRedirect('guides/tasks')).toBeUndefined();
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
