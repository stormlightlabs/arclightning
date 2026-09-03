import { defineConfig } from "@playwright/test";

const theme = (name: "light" | "dark") => ({
  name,
  use: {
    colorScheme: name,
    reducedMotion: "reduce" as const,
    viewport: { width: 1280, height: 1100 },
  },
});

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: Boolean(process.env.CI),
  reporter: "list",
  expect: { toHaveScreenshot: { animations: "disabled", caret: "hide" } },
  use: {
    baseURL: "http://127.0.0.1:6006",
    browserName: "chromium",
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
      ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH }
      : undefined,
    locale: "en-US",
    timezoneId: "UTC",
  },
  projects: [theme("light"), theme("dark")],
  webServer: {
    command: "pnpm storybook --ci --no-open",
    port: 6006,
    reuseExistingServer: !process.env.CI,
  },
});
