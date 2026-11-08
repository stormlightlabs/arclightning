import { createRequire } from 'node:module';
import { expect, test, type Page } from '@playwright/test';

const require = createRequire(import.meta.url);
const AXE = require.resolve('axe-core/axe.min.js');

/** The landing page and two docs pages, the core reading paths. */
const PAGES = ['/', '/quick-start/', '/guides/tasks/'];
const MOBILE = { width: 390, height: 844 };

type AxeViolation = { id: string; impact: string | null; nodes: { target: string[] }[] };

/** Run axe's WCAG 2 A and AA rules on the page and return what fails. */
async function axeViolations(page: Page): Promise<string[]> {
	await page.addScriptTag({ path: AXE });
	const violations = await page.evaluate(async () => {
		const axe = (window as unknown as { axe: { run: (options: object) => Promise<{ violations: unknown[] }> } }).axe;
		const result = await axe.run({ runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa'] } });
		return result.violations as AxeViolation[];
	});
	return violations.map((v) => `${v.id} (${v.impact}): ${v.nodes.map((node) => node.target.join(' ')).join(', ')}`);
}

for (const colorScheme of ['light', 'dark'] as const) {
	test.describe(`axe, ${colorScheme}`, () => {
		test.use({ colorScheme });
		for (const path of PAGES) {
			test(path, async ({ page }) => {
				await page.goto(path);
				expect(await axeViolations(page)).toEqual([]);
			});
		}
	});
}

test.describe('keyboard', () => {
	test('reaches the skip link first and moves to the content', async ({ page }) => {
		await page.goto('/quick-start/');
		await page.keyboard.press('Tab');
		const skip = page.getByRole('link', { name: 'Skip to content' });
		await expect(skip).toBeFocused();
		await expect(skip).toBeInViewport();
		await page.keyboard.press('Enter');
		await expect(page).toHaveURL(/#main-content$/);
	});

	test('searches the built index and opens a result', async ({ page }) => {
		await page.goto('/');
		await page.keyboard.press('Control+k');
		const dialog = page.getByRole('dialog', { name: 'Search the docs' });
		await expect(dialog.getByRole('combobox')).toBeFocused();
		await page.keyboard.type('handoff');
		const results = dialog.getByRole('option');
		await expect(results.first()).toBeVisible();
		await page.keyboard.press('ArrowDown');
		await expect(results.first()).toHaveAttribute('aria-selected', 'true');
		const href = await results.first().getAttribute('href');
		await page.keyboard.press('Enter');
		await expect(page).toHaveURL(new RegExp(`${href}$`));
	});
});

test.describe('reading', () => {
	test('shows the docs shell around the page', async ({ page }) => {
		await page.goto('/guides/tasks/');
		await expect(page.getByRole('heading', { level: 1, name: 'Tasks' })).toBeVisible();
		await expect(page.getByRole('navigation', { name: 'Breadcrumbs' })).toContainText('Guides');
		await expect(
			page.getByRole('complementary', { name: 'Documentation navigation' }).getByRole('link', { name: 'Tasks' })
		).toHaveAttribute('aria-current', 'page');
		await expect(
			page.getByRole('navigation', { name: 'On this page' }).getByRole('link', { name: 'Handoff' })
		).toHaveAttribute('href', '#handoff');
		const pager = page.getByRole('navigation', { name: 'Page navigation' });
		await expect(pager.getByRole('link', { name: /Previous/ })).toBeVisible();
		await expect(pager.getByRole('link', { name: /Next/ })).toBeVisible();
	});

	test('copies the page Markdown and a code block', async ({ page, context }) => {
		await context.grantPermissions(['clipboard-read', 'clipboard-write']);
		await page.goto('/quick-start/');
		await page.getByRole('link', { name: 'Copy Markdown' }).click();
		await expect(page.getByRole('link', { name: 'Copied' })).toBeVisible();
		const markdown = await page.evaluate(() => navigator.clipboard.readText());
		const served = await (await page.request.get('/quick-start.md')).text();
		expect(markdown).toBe(served);

		const block = page.locator('.doc-content pre').first();
		await block.getByRole('button', { name: 'Copy code' }).click();
		expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(await block.locator('code').textContent());
	});

	test('keeps the theme choice across pages', async ({ page }) => {
		await page.goto('/quick-start/');
		await page.getByRole('button', { name: 'Switch to dark mode' }).click();
		await expect(page.locator('html')).toHaveAttribute('data-arcl-theme', 'dark');
		await page.goto('/overview/');
		await expect(page.locator('html')).toHaveAttribute('data-arcl-theme', 'dark');
		await expect(page.getByRole('button', { name: 'Switch to light mode' })).toBeVisible();
	});

	test('smooths scrolling only when motion is allowed', async ({ browser }) => {
		for (const [reducedMotion, expected] of [
			['no-preference', 'smooth'],
			['reduce', 'auto']
		] as const) {
			const page = await browser.newPage({ reducedMotion });
			await page.goto('/quick-start/');
			expect(await page.evaluate(() => getComputedStyle(document.documentElement).scrollBehavior)).toBe(expected);
			await page.close();
		}
	});
});

test.describe('mobile', () => {
	test.use({ viewport: MOBILE });

	for (const path of PAGES) {
		test(`${path} fits the viewport`, async ({ page }) => {
			await page.goto(path);
			expect(await page.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(MOBILE.width);
		});
	}

	test('opens the menu, follows a link and closes the menu', async ({ page }) => {
		await page.goto('/quick-start/');
		await page.getByText('Menu').click();
		const panel = page.locator('.mobile-menu__panel');
		await expect(panel).toBeVisible();
		await panel.getByRole('link', { name: 'Overview' }).click();
		await expect(page).toHaveURL(/\/overview\/$/);
		await expect(panel).toBeHidden();
	});

	test('closes the menu on Escape', async ({ page }) => {
		await page.goto('/quick-start/');
		const toggle = page.getByText('Menu');
		await toggle.click();
		await page.locator('.mobile-menu__panel').getByRole('link', { name: 'Overview' }).focus();
		await page.keyboard.press('Escape');
		await expect(page.locator('.mobile-menu__panel')).toBeHidden();
		await expect(page.getByLabel('Open documentation menu')).toBeFocused();
	});

	test('closes the menu on Escape with nothing focused', async ({ page }) => {
		await page.goto('/quick-start/');
		await page.getByText('Menu').click();
		await expect(page.locator('.mobile-menu__panel')).toBeVisible();
		await page.evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
		await page.keyboard.press('Escape');
		await expect(page.locator('.mobile-menu__panel')).toBeHidden();
		await expect(page.getByLabel('Open documentation menu')).toBeFocused();
	});
});

test.describe('without JavaScript', () => {
	test.use({ javaScriptEnabled: false });

	test('reads a docs page and offers the search page', async ({ page }) => {
		await page.goto('/guides/tasks/');
		await expect(page.getByRole('heading', { level: 1, name: 'Tasks' })).toBeVisible();
		await expect(page.locator('.doc-content pre').first()).toBeVisible();
		await expect(page.getByRole('link', { name: 'Browse documentation' })).toBeVisible();
		await expect(page.getByRole('button', { name: /Open documentation search/ })).toBeHidden();
		await expect(page.getByRole('button', { name: /Switch to/ })).toBeHidden();
		await page.getByRole('navigation', { name: 'Page navigation' }).getByRole('link', { name: /Next/ }).click();
		await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
	});

	test('opens the mobile menu', async ({ page }) => {
		await page.setViewportSize(MOBILE);
		await page.goto('/');
		await page.getByText('Menu').click();
		await expect(page.locator('.mobile-menu__panel').getByRole('link', { name: 'Quick Start' })).toBeVisible();
	});
});

test('serves Markdown, the sitemap and llms.txt', async ({ request }) => {
	const markdown = await request.get('/quick-start.md');
	expect(markdown.ok()).toBe(true);
	expect(await markdown.text()).toContain('title: Quick Start');
	expect(await (await request.get('/sitemap.xml')).text()).toContain('/guides/tasks/</loc>');
	expect(await (await request.get('/llms.txt')).text()).toContain('(/quick-start.md)');
});
