import { expect, test } from '@playwright/test';

const WIDTHS = [
	{ name: 'desktop', viewport: { width: 1280, height: 800 } },
	{ name: 'mobile', viewport: { width: 390, height: 844 } }
] as const;

/** The landing page in full, and the top of two docs pages: a short one and one with tables and code. */
const PAGES = [
	{ name: 'landing', path: '/', fullPage: true },
	{ name: 'quick-start', path: '/tutorials/quick-start/', fullPage: false },
	{ name: 'work-a-task', path: '/how-to/work-a-task/', fullPage: false }
] as const;

for (const width of WIDTHS) {
	for (const colorScheme of ['light', 'dark'] as const) {
		test.describe(`${width.name}, ${colorScheme}`, () => {
			test.use({ viewport: width.viewport, colorScheme, reducedMotion: 'reduce' });

			for (const page of PAGES) {
				test(page.name, async ({ page: browser }) => {
					await browser.goto(page.path);
					await browser.evaluate(() => document.fonts.ready);
					await expect(browser).toHaveScreenshot(`${page.name}-${width.name}-${colorScheme}.png`, {
						fullPage: page.fullPage
					});
				});
			}
		});
	}
}
