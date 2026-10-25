import { playwright } from "@vitest/browser-playwright";
import { sveltekit } from "@sveltejs/kit/vite";
import unoConfig from "@arclightning/ui/uno.config";
import UnoCSS from "unocss/vite";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";

/**
 * Drop `+layout.ts`'s page options from the artifact build. Its hash router
 * never renders on a server, and SvelteKit refuses page options under it,
 * while the `arcl serve` build and `vite dev` need `ssr = false`.
 */
function dropPageOptions(): Plugin {
  return {
    name: "arcl-drop-page-options",
    enforce: "pre",
    transform(_code, id) {
      if (process.env.VITE_ARCL_TARGET === "artifact" && id.endsWith("/src/routes/+layout.ts")) return "export {};";
    },
  };
}

export default defineConfig({
  plugins: [dropPageOptions(), UnoCSS(unoConfig), sveltekit()],
  // Inline fonts and images too, so `index.html` is the app's only file.
  build: { assetsInlineLimit: () => true },
  test: {
    projects: [
      {
        extends: "./vite.config.ts",
        test: {
          name: "unit",
          environment: "node",
          include: ["src/**/*.test.ts"],
          exclude: ["src/**/*.svelte.test.ts", "src/**/*.dom.test.ts"],
        },
      },
      {
        // Components mounted in a DOM need Svelte's client runtime.
        extends: "./vite.config.ts",
        resolve: { conditions: ["browser"] },
        test: { name: "dom", environment: "happy-dom", include: ["src/**/*.dom.test.ts"] },
      },
      {
        extends: "./vite.config.ts",
        test: {
          name: "browser",
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({
              launchOptions: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH
                ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH }
                : undefined,
            }),
            instances: [{ browser: "chromium" }],
            viewport: { width: 1024, height: 768 },
          },
          include: ["src/**/*.svelte.test.ts"],
          setupFiles: ["vitest-browser-svelte"],
        },
      },
    ],
  },
});
