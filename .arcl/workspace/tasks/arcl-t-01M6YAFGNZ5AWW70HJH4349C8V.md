+++
id = "arcl-t-01M6YAFGNZ5AWW70HJH4349C8V"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "Deploy the site to arcl.stormlightlabs.org on Cloudflare Pages"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M6YAFBCE29665W3ZHAVA5F18"]
+++

Publish the static site to arcl.stormlightlabs.org on Cloudflare Pages from
`main`.

Acceptance criteria:

- [ ] Add a GitHub Actions workflow that builds `apps/website` and runs
      `wrangler pages deploy` on each push to `main`
- [ ] Read the Cloudflare credentials from the repository secrets
      `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
- [ ] Attach arcl.stormlightlabs.org to the Pages project as a custom domain in
      Cloudflare

Verification:

- The workflow run on `main` succeeds and reports the deployment URL
- `https://arcl.stormlightlabs.org/`, a docs page, `/sitemap.xml` and
  `/llms.txt` load over HTTPS
