import adapter from '@sveltejs/adapter-static';
import { playwright } from '@vitest/browser-playwright';
import { sveltekit } from '@sveltejs/kit/vite';
import { escapeSvelte, mdsvex } from 'mdsvex';
import rehypeSlug from 'rehype-slug';
import { createHighlighter } from 'shiki';
import { defineConfig } from 'vitest/config';
import { extractTableOfContents } from './src/lib/table-of-contents.ts';

const highlighter = await createHighlighter({
	themes: ['github-light-default', 'github-dark-default'],
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
										themes: { light: 'github-light-default', dark: 'github-dark-default' },
										defaultColor: false
									})
									.replace(' tabindex="0"', '')
							)
					},
					rehypePlugins: [rehypeSlug, extractTableOfContents]
				})
			]
		})
	],
	test: {
		projects: [
			{
				extends: './vite.config.ts',
				test: { name: 'unit', environment: 'node', include: ['src/**/*.test.ts'], exclude: ['src/**/*.svelte.test.ts'] }
			},
			{
				extends: './vite.config.ts',
				test: {
					name: 'browser',
					browser: {
						enabled: true,
						headless: true,
						provider: playwright({
							launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
								? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH }
								: undefined
						}),
						instances: [{ browser: 'chromium' }],
						viewport: { width: 1280, height: 800 }
					},
					include: ['src/**/*.svelte.test.ts'],
					setupFiles: ['vitest-browser-svelte']
				}
			}
		]
	}
});
