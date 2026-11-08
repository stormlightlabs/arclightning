import { page as screen, userEvent } from 'vitest/browser';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import CopyMarkdown from '../components/CopyMarkdown.svelte';
import ThemeToggle from '../components/ThemeToggle.svelte';
import CopyCodeHarness from './CopyCodeHarness.svelte';
import TableOfContentsHarness from './TableOfContentsHarness.svelte';

/** Replace the clipboard with a spy, since headless Chromium denies it. */
function stubClipboard(write: (text: string) => Promise<void> = async () => {}) {
	const writeText = vi.fn(write);
	vi.spyOn(navigator, 'clipboard', 'get').mockReturnValue({ writeText } as unknown as Clipboard);
	return writeText;
}

afterEach(() => {
	vi.restoreAllMocks();
	localStorage.clear();
	delete document.documentElement.dataset.arclTheme;
	document.querySelector('meta[name="theme-color"]')?.remove();
});

describe('ThemeToggle', () => {
	beforeEach(() => {
		const meta = document.createElement('meta');
		meta.name = 'theme-color';
		document.head.append(meta);
	});

	it('switches the theme, stores the choice and updates the browser color', async () => {
		localStorage.setItem('arcl-theme', 'light');
		render(ThemeToggle);
		const toggle = screen.getByRole('button', { name: 'Switch to dark mode' });
		await expect.element(toggle).toHaveAttribute('aria-pressed', 'false');

		await toggle.click();
		const dark = screen.getByRole('button', { name: 'Switch to light mode' });
		await expect.element(dark).toHaveAttribute('aria-pressed', 'true');
		expect(document.documentElement.dataset.arclTheme).toBe('dark');
		expect(localStorage.getItem('arcl-theme')).toBe('dark');
		expect(document.querySelector('meta[name="theme-color"]')?.getAttribute('content')).toBe('#09090b');

		await dark.click();
		await expect.element(screen.getByRole('button', { name: 'Switch to dark mode' })).toBeVisible();
		expect(document.documentElement.dataset.arclTheme).toBe('light');
		expect(localStorage.getItem('arcl-theme')).toBe('light');
	});

	it('starts from the stored theme', async () => {
		localStorage.setItem('arcl-theme', 'dark');
		render(ThemeToggle);
		await expect.element(screen.getByRole('button', { name: 'Switch to light mode' })).toBeVisible();
	});
});

describe('CopyMarkdown', () => {
	it('copies the page Markdown and links to the .md file', async () => {
		const writeText = stubClipboard();
		render(CopyMarkdown, { markdown: '# Quick start\n', slug: 'quick-start' });
		const link = screen.getByRole('link', { name: 'Copy Markdown' });
		await expect.element(link).toHaveAttribute('href', '/quick-start.md');

		await link.click();
		expect(writeText).toHaveBeenCalledWith('# Quick start\n');
		await expect.element(screen.getByRole('link', { name: 'Copied' })).toBeVisible();
	});
});

describe('CopyCode', () => {
	it('adds a copy button to each code block', async () => {
		const writeText = stubClipboard();
		render(CopyCodeHarness);
		const buttons = screen.getByRole('button', { name: 'Copy code' });
		await expect.element(buttons.nth(1)).toBeVisible();
		expect(buttons.all()).toHaveLength(2);

		await buttons.nth(1).click();
		expect(writeText).toHaveBeenCalledWith('arcl ready');
		await expect.element(screen.getByRole('button', { name: 'Code copied' })).toHaveTextContent('Copied');
	});

	it('says when the copy fails', async () => {
		stubClipboard(async () => {
			throw new Error('denied');
		});
		render(CopyCodeHarness);
		await screen.getByRole('button', { name: 'Copy code' }).first().click();
		await expect.element(screen.getByText('Copy failed')).toBeVisible();
	});
});

describe('TableOfContents', () => {
	const headings = [
		{ title: 'Install', slug: 'install', level: 2 as const },
		{ title: 'From source', slug: 'from-source', level: 3 as const },
		{ title: 'Next steps', slug: 'next-steps', level: 2 as const }
	];

	it('links each heading and marks the one in view', async () => {
		render(TableOfContentsHarness, { headings });
		const nav = screen.getByRole('navigation', { name: 'On this page' });
		await expect.element(nav.getByRole('link', { name: 'From source' })).toHaveAttribute('href', '#from-source');
		await expect.element(nav.getByRole('listitem').nth(1)).toHaveClass('toc__subitem');

		// The observer marks a heading in the band from 20% to 30% of the viewport height.
		const heading = document.getElementById('next-steps')!;
		window.scrollTo(0, heading.getBoundingClientRect().top + window.scrollY - window.innerHeight * 0.25);
		await expect.element(nav.getByRole('link', { name: 'Next steps' })).toHaveAttribute('aria-current', 'location');
	});

	it('renders nothing for a page without headings', async () => {
		render(TableOfContentsHarness, { headings: [] });
		await expect.element(screen.getByRole('navigation', { name: 'On this page' })).not.toBeInTheDocument();
	});
});

describe('keyboard', () => {
	it('reaches the theme toggle with Tab and switches it with Enter', async () => {
		render(ThemeToggle);
		await userEvent.keyboard('{Tab}');
		await expect.element(screen.getByRole('button', { name: /Switch to/ })).toHaveFocus();
		const before = document.documentElement.dataset.arclTheme;
		await userEvent.keyboard('{Enter}');
		expect(document.documentElement.dataset.arclTheme).not.toBe(before);
	});
});
