/** @type {import("prettier").Config} */
const config = {
	useTabs: true,
	bracketSameLine: true,
	singleQuote: true,
	trailingComma: 'none',
	printWidth: 120,
	plugins: ['prettier-plugin-svelte'],
	overrides: [
		{ files: '*.svelte', options: { parser: 'svelte' } },
		// Code blocks in the docs show command output as `arcl` prints it.
		{ files: '*.md', options: { embeddedLanguageFormatting: 'off' } }
	]
};

export default config;
