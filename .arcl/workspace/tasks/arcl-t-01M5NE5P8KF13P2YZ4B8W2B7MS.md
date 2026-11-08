+++
id = "arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "T14: Rebuild the website and documentation shell"
status = "completed"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5P6XHRR4Q84820YACQ07", "arcl-t-01M5NH1N3R9AX3XZH6XG9ZX1QE"]
evidence = "Done in #95: the website shell takes its colors from the packages/ui grey and blue tokens and its fonts (IBM Plex Sans, Google Sans, Google Sans Code) from Fontsource under the OFL. The mobile menu closes on Escape and after navigation. Vitest browser tests cover the theme toggle, copy controls, table of contents, search dialog and menu. Playwright covers axe WCAG A/AA in both themes, keyboard, search, copy, reduced motion, mobile fit and the no-JavaScript path, with screenshots of the landing page, Quick Start and Tasks at 1280 and 390 px in both themes. The Playwright e2e and visual tests run locally, and their screenshot baselines are not committed. CI runs the website build through `just check`, so a page that fails to prerender fails CI."
completed-at = "2026-11-08T19:47:52.812Z"
+++

Build the website and documentation shell in `apps/website`: the landing page,
the docs layout and its controls, the theme, and the tests. The existing pages
render in the shell as they are. Writing and migrating the content, the
vocabulary update and redirects belong to "Write the documentation content".
Deployment belongs to "Deploy the site to arcl.stormlightlabs.org on
Cloudflare Pages".

Blocked by: T11, T18

Acceptance criteria:

- [x] Build the landing page with a concrete Arc Lightning workflow example
- [x] Add the sticky header, grouped sidebar, breadcrumbs, article layout,
      on-page table of contents, previous/next links, responsive menu, search,
      and theme control
- [x] Implement the shell directly in `apps/website`, using the shared UI
      package only for reusable primitives
- [x] Use IBM Plex Sans for body text, Google Sans for headings and controls,
      and Google Sans Code for code, self-hosted through Fontsource under the
      SIL Open Font License
- [x] Take every color from the `packages/ui` grey and blue tokens in both
      themes, with no hardcoded navy or gold
- [x] Add copy-code, copy-Markdown, syntax highlighting in both themes, page
      and social metadata, a sitemap, `llms.txt`, and static output
- [x] Pass keyboard, contrast, responsive, reduced-motion, and no-JavaScript
      checks for the landing page and docs pages
- [x] Test interactive Svelte components through Vitest browser mode with the
      Playwright provider and `vitest-browser-svelte`
- [x] Add Playwright screenshot coverage for the landing page and two docs
      pages at fixed mobile and desktop viewports in both themes

Verification:

- `pnpm --filter @arclightning/website check`
- `pnpm --filter @arclightning/website lint`
- `pnpm --filter @arclightning/website test`
- `pnpm --filter @arclightning/website test:e2e`
- `pnpm --filter @arclightning/website test:visual`
- `pnpm --filter @arclightning/website build`
- Review the Playwright screenshots at mobile and desktop widths in both themes
