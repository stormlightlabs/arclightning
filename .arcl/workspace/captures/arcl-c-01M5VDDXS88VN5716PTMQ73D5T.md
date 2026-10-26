+++
id = "arcl-c-01M5VDDXS88VN5716PTMQ73D5T"
title = "Fix the Prettier failure in apps/website"
status = "captured"
created-at = "2026-10-26T06:12:41.896Z"
+++

On main, `pnpm lint` in apps/website fails on `src/content/docs/reference/manual.md`, so its lint stays out of the root `pnpm lint` and `just check`. Format the file, then add apps/website to the root `lint` and `format` scripts.
