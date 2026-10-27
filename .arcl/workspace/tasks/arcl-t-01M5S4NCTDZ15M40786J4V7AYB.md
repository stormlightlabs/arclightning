+++
id = "arcl-t-01M5S4NCTDZ15M40786J4V7AYB"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T25: Enable bracketSameLine for apps/web and packages/ui"
status = "completed"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5S4N8CBY1JSE73MV0NN5BW4"]
evidence = "Delivered by #71: apps/web and packages/ui set `bracketSameLine: true`, and one commit reformats both packages with no other changes. Verified with pnpm lint, check and test, the web build, build:artifact, and cargo fmt, clippy and test."
+++

Set `bracketSameLine: true` in the Prettier config for `apps/web` and
`packages/ui`, as `apps/website` already does, and reformat, so a
multi-line tag's closing `>` ends its last attribute line.

Blocked by: T24

Acceptance criteria:

- [x] The Prettier config for `apps/web` and `packages/ui` sets
      `bracketSameLine: true`
- [x] Both packages are reformatted in one commit, with no other changes

Verification:

- `pnpm lint`, `pnpm check`, `pnpm test`
- `just check`
