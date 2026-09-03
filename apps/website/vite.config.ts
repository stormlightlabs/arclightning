import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { escapeSvelte, mdsvex } from 'mdsvex';
import rehypeSlug from 'rehype-slug';
import { createHighlighter } from 'shiki';
import { defineConfig } from 'vite';
import { extractTableOfContents } from './src/lib/table-of-contents.ts';

const highlighter = await createHighlighter({
	themes: ['github-light', 'github-dark'],
	langs: ['shellscript', 'text', 'toml', 'json']
});

function languageFor(language: string): 'shellscript' | 'text' | 'toml' | 'json' {
	if (language === 'sh' || language === 'bash' || language === 'shell') return 'shellscript';
	if (language === 'toml' || language === 'json' || language === 'text') return language;
	return 'text';
}

export default defineConfig({
	plugins: [
		sveltekit({
			adapter: adapter(),
			extensions: ['.svelte', '.md'],
			preprocess: [
				mdsvex({
					extensions: ['.md'],
					highlight: {
						highlighter: (code, language) =>
							escapeSvelte(
								highlighter
									.codeToHtml(code, {
										lang: languageFor(language ?? 'text'),
										themes: { light: 'github-light', dark: 'github-dark' },
										defaultColor: false
									})
									.replace(' tabindex="0"', '')
							)
					},
					rehypePlugins: [rehypeSlug, extractTableOfContents]
				})
			]
		})
	]
});
