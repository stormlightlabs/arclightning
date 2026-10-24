import { sveltekit } from "@sveltejs/kit/vite";
import unoConfig from "@arclightning/ui/uno.config";
import UnoCSS from "unocss/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [UnoCSS(unoConfig), sveltekit()],
  // Inline fonts and images too, so `index.html` is the app's only file.
  build: { assetsInlineLimit: () => true },
  test: {
    projects: [
      {
        extends: true,
        test: { name: "unit", environment: "node", include: ["src/**/*.test.ts"], exclude: ["src/**/*.dom.test.ts"] },
      },
      {
        // Components mounted in a DOM need Svelte's client runtime.
        extends: true,
        resolve: { conditions: ["browser"] },
        test: { name: "dom", environment: "happy-dom", include: ["src/**/*.dom.test.ts"] },
      },
    ],
  },
});
