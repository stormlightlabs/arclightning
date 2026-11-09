+++
id = "arcl-t-01M6YAFBCE29665W3ZHAVA5F18"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "Write the documentation content"
status = "completed"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS", "arcl-t-01M6YVG52P5TSEFQ415C9Q668B"]
evidence = "PR #97 writes the documentation content: 23 pages, redirects for the three retired guides (HTML keeps the fragment, `.md` serves the replacement), old section anchors kept, titles and descriptions in the sitemap and `llms.txt`. Commands checked against `--help`, MCP tools against `tools/list`, GraphQL examples run. Website build, lint, check, test and test:e2e pass."
completed-at = "2026-11-09T04:20:54.618Z"
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
- [x] Document the `arcl` CLI, the web app, the MCP server, the GraphQL API
      with `arcl serve --api`, and the repository-native workflows. The manual
      and guides cover all 86 leaf commands (100 command paths under `arcl`),
      each command and flag checked against its `--help`; new guides: agents,
      api, artifact, captures, compaction, mcp, notes, phases, plans, rest,
      specs and web-app. The 21 MCP tools and their format versions match
      `tools/list` from `arcl-mcp`, and the GraphQL and `curl` examples in
      the API guide run against a scratch project
- [x] Replace the idea, epic and milestone commands and guides with capture,
      spec, plan and phase, as capture arcl-c-01M62S04FHX7WVC9N5JH0RX3N9
      ("Replace the Idea, Epic and Milestone commands in the manual") asks.
      The manual's command sections and the captures, specs, plans and phases
      guides replace them
- [x] Keep each useful existing docs URL, or add a redirect from it to the page
      that replaces it. `/guides/ideas/`, `/guides/epics/` and
      `/guides/milestones/` redirect to captures, specs and phases and keep
      the `#fragment`; their `.md` URLs serve the replacement Markdown. The
      manual and automation pages keep ids for their old section anchors
- [x] Give every page a title and description for its metadata, the sitemap
      and `llms.txt`. All 23 pages have both; the build's `sitemap.xml` lists
      the landing page and the 23 pages, and `llms.txt` lists the 23 with
      their descriptions

Verification:

- Dispatch Preview on this branch and open the URLs in the run summary
- `pnpm --filter @arclightning/website build`, which prerenders every page
  and redirect
- `pnpm --filter @arclightning/website lint`
- `pnpm --filter @arclightning/website test:e2e`
- Check each documented command against `arcl --help` for that command
