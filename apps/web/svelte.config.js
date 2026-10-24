import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/**
 * `arcl serve` checks a token on every request, and a browser sends no custom
 * header for a script, stylesheet or font it loads itself. The `inline`
 * bundle strategy puts the whole app in `index.html`, so the page is the only
 * file the browser fetches outside the API.
 *
 * @type {import("@sveltejs/kit").Config}
 */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter({ fallback: "index.html" }),
    output: { bundleStrategy: "inline" },
  },
};

export default config;
