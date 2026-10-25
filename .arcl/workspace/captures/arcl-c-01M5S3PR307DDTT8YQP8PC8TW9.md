+++
id = "arcl-c-01M5S3PR307DDTT8YQP8PC8TW9"
title = "Stop the first web browser test run after an install from reloading"
status = "captured"
created-at = "2026-10-25T08:44:16.352Z"
+++

On a fresh worktree, the first `pnpm --filter @arclightning/web test:browser` found the codemirror and micromark packages partway through, Vite reloaded the page, and 32 route tests failed with `effect_orphan`. The next run passed. Listing them in `optimizeDeps.include` from apps/web does not resolve under pnpm (they are packages/ui dependencies), and `@arclightning/ui > name` builds a second copy. Found in T23.
