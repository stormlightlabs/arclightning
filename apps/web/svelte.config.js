import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/**
 * Whether this is the claude.ai artifact build, which `build:artifact` asks
 * for with `VITE_ARCL_TARGET=artifact`.
 */
const artifact = process.env.VITE_ARCL_TARGET === "artifact";

/**
 * `arcl serve` checks a token on every request, and a browser sends no custom
 * header for a script, stylesheet or font it loads itself. The `inline`
 * bundle strategy puts the whole app in `index.html`, so the page is the only
 * file the browser fetches outside the API.
 *
 * The artifact build is the same single page, written to `build-artifact/`.
 * The artifact host serves it at a path of its own, so it routes on the URL's
 * hash instead of its path. It keeps its generated files in
 * `.svelte-kit/artifact/`, so the two builds can run at once.
 *
 * @type {import("@sveltejs/kit").Config}
 */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: artifact
      ? adapter({ pages: "build-artifact", assets: "build-artifact", fallback: "index.html" })
      : adapter({ fallback: "index.html" }),
    outDir: artifact ? ".svelte-kit/artifact" : ".svelte-kit",
    output: { bundleStrategy: "inline" },
    router: { type: artifact ? "hash" : "pathname" },
  },
};

export default config;
