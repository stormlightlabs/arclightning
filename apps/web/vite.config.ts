import { sveltekit } from "@sveltejs/kit/vite";
import unoConfig from "@arclightning/ui/uno.config";
import UnoCSS from "unocss/vite";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [UnoCSS(unoConfig), sveltekit()],
  // Inline fonts and images too, so `index.html` is the app's only file.
  build: { assetsInlineLimit: () => true },
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  },
});
