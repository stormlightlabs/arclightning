+++
id = "arcl-c-01M5VDDXS88VN5716PTMQ73D5T"
title = "Fix the Prettier failure in apps/website"
status = "discarded"
created-at = "2026-10-26T06:12:41.896Z"
+++

On main, `pnpm lint` in apps/website fails on `src/content/docs/reference/manual.md`, so its lint stays out of the root `pnpm lint` and `just check`. Format the file, then add apps/website to the root `lint` and `format` scripts.

Fixed in this PR: the website passes Prettier, and the root `lint` and `format` scripts include apps/website. Prettier leaves code blocks in the docs alone, so sample output stays as `arcl` prints it.
