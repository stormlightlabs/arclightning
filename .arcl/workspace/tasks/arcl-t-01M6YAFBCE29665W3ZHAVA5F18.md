+++
id = "arcl-t-01M6YAFBCE29665W3ZHAVA5F18"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "Write the documentation content"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS", "arcl-t-01M6YVG52P5TSEFQ415C9Q668B"]
+++

Write the documentation that the shell from "Rebuild the website and
documentation shell" presents, in `apps/website`, so the site documents the
CLI, the web app, MCP, the GraphQL API and the repository workflows in the
current product vocabulary.

Acceptance criteria:

- [x] The first dispatched Preview run deploys this task's branch, the first
      one to preview. It needs `.github/workflows/preview.yml` on `main` and
      the `CLOUDFLARE_PAGES_API_TOKEN` and `CLOUDFLARE_PAGES_API_ACCOUNT_ID`
      organization secrets. Run
      https://github.com/stormlightlabs/arclightning/actions/runs/42558302137
      deployed https://claude-docs-content.arclightning.pages.dev (deployment
      https://18fa9128.arclightning.pages.dev). `/`, `/guides/captures/`,
      `/og.png`, `/llms.txt` and `/sitemap.xml` return 200, `/guides/ideas/`
      meta-refreshes to `/guides/captures/`, and the preview sends
      `x-robots-tag: noindex`
- [ ] Document the `arcl` CLI, the web app, the MCP server, the GraphQL API
      with `arcl serve --api`, and the repository-native workflows
- [ ] Replace the idea, epic and milestone commands and guides with capture,
      spec, plan and phase, as capture arcl-c-01M62S04FHX7WVC9N5JH0RX3N9
      ("Replace the Idea, Epic and Milestone commands in the manual") asks
- [ ] Keep each useful existing docs URL, or add a redirect from it to the page
      that replaces it
- [ ] Give every page a title and description for its metadata, the sitemap
      and `llms.txt`

Verification:

- Dispatch Preview on this branch and open the URLs in the run summary
- `pnpm --filter @arclightning/website build`, which prerenders every page
  and redirect
- `pnpm --filter @arclightning/website lint`
- `pnpm --filter @arclightning/website test:e2e`
- Check each documented command against `arcl --help` for that command
