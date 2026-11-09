+++
id = "arcl-t-01M6YAFGNZ5AWW70HJH4349C8V"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "Deploy the site to arcl.stormlightlabs.org on Cloudflare Pages"
status = "pending"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M6YAFBCE29665W3ZHAVA5F18"]
+++

Publish the static site to arcl.stormlightlabs.org on Cloudflare Pages from
`main`. Branch previews belong to "Preview the website from any branch on
Cloudflare Pages", which also creates the Pages project.

Acceptance criteria:

- [ ] Add a deploy workflow that runs on each push to `main` and on manual
      dispatch, and refuses a dispatch from any other branch
- [ ] Reuse the preview workflow's install and build steps, through a shared
      composite action or reusable workflow, so the two cannot drift
- [ ] Deploy `apps/website/build` to the `arclightning` project's production
      branch, `main`
- [ ] Attach arcl.stormlightlabs.org to the Pages project as a custom domain in
      Cloudflare

Verification:

- The workflow run on `main` succeeds and reports the deployment URL
- `https://arcl.stormlightlabs.org/`, a docs page, `/sitemap.xml` and
  `/llms.txt` load over HTTPS
