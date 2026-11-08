import { describe, expect, it } from 'vitest';
import { extractTableOfContents } from './table-of-contents';

describe('extractTableOfContents', () => {
	it('adds h2 and h3 headings with ids to the front matter', () => {
		const tree = {
			type: 'root',
			children: [
				{ type: 'element', tagName: 'h1', properties: { id: 'title' }, children: [{ value: 'Title' }] },
				{ type: 'element', tagName: 'h2', properties: { id: 'install' }, children: [{ value: 'Install' }] },
				{
					type: 'element',
					tagName: 'h3',
					properties: { id: 'from-source' },
					children: [{ value: 'From ' }, { type: 'element', tagName: 'code', children: [{ value: 'source' }] }]
				},
				{ type: 'element', tagName: 'h2', properties: {}, children: [{ value: 'No id' }] }
			]
		};
		const file = { data: { fm: { title: 'Quick start' } } as Record<string, unknown> };
		extractTableOfContents()(tree, file);
		expect(file.data.fm).toEqual({
			title: 'Quick start',
			toc: [
				{ title: 'Install', slug: 'install', level: 2 },
				{ title: 'From source', slug: 'from-source', level: 3 }
			]
		});
	});

	it('leaves a page without front matter alone', () => {
		const file = { data: {} as Record<string, unknown> };
		extractTableOfContents()({ type: 'root', children: [] }, file);
		expect(file.data).toEqual({});
	});
});
