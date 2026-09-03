import adapter from "@sveltejs/adapter-static";
import { sveltekit } from "@sveltejs/kit/vite";
import { mdsvex } from "mdsvex";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [
    sveltekit({
      adapter: adapter(),
      extensions: [".svelte", ".md"],
      preprocess: [mdsvex({ extensions: [".md"] })],
    }),
  ],
});
