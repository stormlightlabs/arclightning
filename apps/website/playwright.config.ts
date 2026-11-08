import { defineConfig } from '@playwright/test';

/** The `vite preview` port. Set `WEBSITE_E2E_PORT` to run beside another checkout. */
const PORT = Number(process.env.WEBSITE_E2E_PORT ?? 4175);

export default defineConfig({
	testDir: './e2e',
	fullyParallel: true,
	forbidOnly: Boolean(process.env.CI),
	reporter: 'list',
	expect: { toHaveScreenshot: { animations: 'disabled', caret: 'hide' } },
	use: {
		baseURL: `http://127.0.0.1:${PORT}`,
		browserName: 'chromium',
		launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
			? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH }
			: undefined,
		locale: 'en-US',
		timezoneId: 'UTC',
		viewport: { width: 1280, height: 800 }
	},
	projects: [
		{ name: 'e2e', testMatch: /\.e2e\.ts$/ },
		{ name: 'visual', testMatch: /\.visual\.ts$/ }
	],
	// `pnpm test:e2e` and `pnpm test:visual` build the site first.
	webServer: {
		command: `pnpm preview --host 127.0.0.1 --port ${PORT} --strictPort`,
		port: PORT,
		reuseExistingServer: !process.env.CI
	}
});
