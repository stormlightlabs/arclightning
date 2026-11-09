+++
id = "arcl-t-01M6YVG52P5TSEFQ415C9Q668B"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "Preview the website from any branch on Cloudflare Pages"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS"]
evidence = 'Built in PR #96. `.github/workflows/preview.yml` runs on `workflow_dispatch` only and refuses `main`. A `build` job with no secrets installs with a frozen lockfile, builds `apps/website` and uploads `apps/website/build` as an artifact. A `deploy` job with no checkout or install downloads it, creates the `arclightning` Pages project if missing with `npx wrangler@4.158.0`, deploys with `cloudflare/wrangler-action@v4` (`wranglerVersion: 4.158.0`, `pages deploy apps/website/build --project-name=arclightning --branch=<branch>`) and writes the deployment and alias URLs to the run summary. Only that job gets the Cloudflare secrets. The README documents it. actionlint passes. The first dispatched run moved to "Write the documentation content".'
completed-at = "2026-11-09T00:48:38.998Z"
+++

Let anyone with write access deploy `apps/website` from a branch to a
Cloudflare Pages preview, so documentation changes can be read on the real
host before they merge. Production stays with "Deploy the site to
arcl.stormlightlabs.org on Cloudflare Pages".

Acceptance criteria:

- [x] `.github/workflows/preview.yml` runs only on `workflow_dispatch`, with
      no push or pull request trigger
- [x] Its first step fails on `main` with "Preview deploys refuse main; pick
      another branch."
- [x] It installs with a frozen lockfile and builds the site with
      `pnpm --filter @arclightning/website build` into `apps/website/build`,
      in a job that gets no secrets
- [x] A separate deploy job, with no checkout or install, runs
      `pages deploy apps/website/build --project-name=arclightning
      --branch=<branch>` on the built site through `cloudflare/wrangler-action`
      with a pinned wrangler, and only it gets the secrets
      `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
- [x] It creates the `arclightning` Pages project, production branch `main`,
      when the project does not exist, and goes on when it does
- [x] It writes the deployment URL and the branch alias URL to the run summary
- [x] It has `contents: read` permissions and one concurrency group per branch
- [x] The root README says how to run a preview, that the branch needs the
      workflow file, and that `main` is refused

The first dispatched run is the first criterion of "Write the documentation
content".

Verification:

- `actionlint .github/workflows/preview.yml`
- Input and output names checked against `cloudflare/wrangler-action`'s
  `action.yml`
