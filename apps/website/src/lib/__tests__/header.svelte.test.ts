import { page as screen, userEvent } from 'vitest/browser';
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { render } from 'vitest-browser-svelte';
import SiteHeader from '../components/SiteHeader.svelte';

/** The callbacks SiteHeader registered with `afterNavigate`. */
const navigated = vi.hoisted(() => [] as (() => void)[]);
vi.mock('$app/navigation', () => ({ afterNavigate: (callback: () => void) => navigated.push(callback) }));

/** The site's search script, which wires up each search box it finds. A variable keeps svelte-check off the untyped file. */
const searchScript = '../../../static/search.js';

beforeAll(async () => {
	await import(/* @vite-ignore */ searchScript);
});

/** The menu's `<details>` element. */
function menu(): HTMLDetailsElement {
	return document.querySelector('details.mobile-menu')!;
}

describe('search', () => {
	it('opens from its button, focuses the field and returns focus on Escape', async () => {
		render(SiteHeader, { docs: [] });
		const trigger = screen.getByRole('button', { name: /Open documentation search/ }).first();
		await trigger.click();
		const dialog = screen.getByRole('dialog', { name: 'Search the docs' }).first();
		await expect.element(dialog).toBeVisible();
		await expect.element(dialog.getByRole('combobox')).toHaveFocus();

		await userEvent.keyboard('{Escape}');
		await expect.poll(() => document.querySelector<HTMLDialogElement>('#header-search-dialog')?.open).toBe(false);
		await expect.element(trigger).toHaveFocus();
		await expect.element(trigger).toHaveAttribute('aria-expanded', 'false');
	});

	it('opens with Ctrl+K', async () => {
		render(SiteHeader, { docs: [] });
		await userEvent.keyboard('{Control>}k{/Control}');
		await expect.element(screen.getByRole('dialog', { name: 'Search the docs' }).first()).toBeVisible();
		await userEvent.keyboard('{Escape}');
		await expect.poll(() => document.querySelector<HTMLDialogElement>('#header-search-dialog')?.open).toBe(false);
	});
});

describe('menu', () => {
	const docs = [{ slug: 'overview', title: 'Overview' }] as Parameters<typeof SiteHeader>[1]['docs'];

	it('closes on Escape and returns focus to its toggle', async () => {
		render(SiteHeader, { docs, currentSlug: 'overview' });
		const toggle = screen.getByLabelText('Open documentation menu');
		await toggle.click();
		expect(menu().open).toBe(true);
		await expect.element(screen.getByRole('link', { name: 'Overview' })).toHaveClass('active');

		(screen.getByRole('link', { name: 'Overview' }).element() as HTMLElement).focus();
		await userEvent.keyboard('{Escape}');
		expect(menu().open).toBe(false);
		await expect.element(toggle).toHaveFocus();
	});

	it('closes after a navigation', async () => {
		render(SiteHeader, { docs });
		await screen.getByLabelText('Open documentation menu').click();
		expect(menu().open).toBe(true);
		navigated.at(-1)?.();
		expect(menu().open).toBe(false);
	});
});
