// `arcl serve` serves the built app as one static page. The artifact build's
// hash router never renders on a server and refuses page options, so
// `vite.config.ts` drops this module from it. `as boolean` keeps SvelteKit
// from reading the value from the source, which would skip that step.
export const ssr = false as boolean;
