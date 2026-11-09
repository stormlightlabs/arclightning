+++
id = "arcl-t-01M6YVG52P5TSEFQ415C9Q668B"
phase = "arcl-ph-01M5NE5P7R5JW95BAQ5J7EG6D4"
title = "Preview the website from any branch on Cloudflare Pages"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS"]
evidence = "Built in PR #96: `.github/workflows/preview.yml` runs on `workflow_dispatch` only, refuses `main`, builds `apps/website` with a frozen lockfile, creates the `arclightning` Pages project if missing, deploys with `cloudflare/wrangler-action@v4` (`pages deploy apps/website/build --project-name=arclightning --branch=<branch>`) and writes the deployment and alias URLs to the run summary. The README documents it. actionlint passes. Left: the first dispatched run. GitHub lists a `workflow_dispatch` workflow in Actions only once the file is on `main`, so it can run only after the merge, and it needs `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`."
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
      `pnpm --filter @arclightning/website build` into `apps/website/build`
- [x] It runs `pages deploy apps/website/build --project-name=arclightning
      --branch=<branch>` through `cloudflare/wrangler-action`, with the
      secrets `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`
- [x] It creates the `arclightning` Pages project, production branch `main`,
      when the project does not exist, and goes on when it does
- [x] It writes the deployment URL and the branch alias URL to the run summary
- [x] It has `contents: read` permissions and one concurrency group per branch
- [x] The root README says how to run a preview, that the branch needs the
      workflow file, and that `main` is refused
- [ ] The first dispatched run deploys a preview. It needs the owner to add
      the two secrets, and the workflow on `main`, since GitHub lists a
      `workflow_dispatch` workflow only once it is on the default branch.

Verification:

- `actionlint .github/workflows/preview.yml`
- Input and output names checked against `cloudflare/wrangler-action`'s
  `action.yml`
- Dispatch Preview on a branch and open the URL in the run summary
