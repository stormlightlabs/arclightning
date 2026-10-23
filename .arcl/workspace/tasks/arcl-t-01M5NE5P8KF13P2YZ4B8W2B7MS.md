+++
id = "arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "T14: Rebuild the website and documentation shell"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P6XHRR4Q84820YACQ07"]
+++

Use the shared UI package for the product website and implement Arc Lightning's
version of the Stormlight documentation house style.

Blocked by: T11, T18

Acceptance criteria:

- [ ] Replace the current documentation presentation only after the new static
      site builds and all retained content has a destination
- [ ] Build the landing page with shared UI components and a concrete Arc
      Lightning workflow example
- [ ] Add the sticky header, grouped sidebar, breadcrumbs, article layout,
      on-page table of contents, previous/next links, responsive menu, search,
      and theme control used by the sibling documentation sites
- [ ] Implement the documentation shell directly in `apps/website`, using the
      shared UI package only for reusable primitives
- [ ] Use IBM Plex Sans, Google Sans, and Google Sans Code with Arc Lightning's
      navy/sky and gold/amber/yellow theme
- [ ] Add copy-code, copy-Markdown, syntax highlighting, social metadata,
      sitemap, `llms.txt`, and static output
- [ ] Preserve useful documentation URLs or add redirects and update content for
      the new product vocabulary
- [ ] Pass keyboard, contrast, responsive, reduced-motion, and no-JavaScript
      checks for core reading paths
- [ ] Test interactive Svelte components through Vitest browser mode with the
      Playwright provider and `vitest-browser-svelte`
- [ ] Add Playwright screenshot coverage for the landing page and representative
      documentation pages at fixed mobile and desktop viewports in both themes

Verification:

- `pnpm --filter @arclightning/website check`
- `pnpm --filter @arclightning/website test`
- `pnpm --filter @arclightning/website test:browser`
- `pnpm --filter @arclightning/website test:e2e`
- `pnpm --filter @arclightning/website test:visual`
- `pnpm --filter @arclightning/website build`
- Review Playwright screenshot diffs for mobile and desktop widths in both
  themes
