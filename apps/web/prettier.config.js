/**
 * Prettier settings for this package. They keep the code's two-space indent,
 * double quotes and trailing commas, and take `printWidth` and the Svelte
 * plugin from `apps/website`.
 *
 * @type {import("prettier").Config}
 */
const config = {
  printWidth: 120,
  plugins: ["prettier-plugin-svelte"],
  overrides: [{ files: "*.svelte", options: { parser: "svelte" } }],
};

export default config;
