+++
id = "arcl-t-01M5S4NCTDZ15M40786J4V7AYB"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T25: Enable bracketSameLine for apps/web and packages/ui"
status = "pending"
priority = "normal"
position = 4
blocked-by = ["arcl-t-01M5S4N8CBY1JSE73MV0NN5BW4"]
+++

Set `bracketSameLine: true` in the Prettier config for `apps/web` and
`packages/ui`, as `apps/website` already does, and reformat, so a
multi-line tag's closing `>` ends its last attribute line.

Blocked by: T24

Acceptance criteria:

- [ ] The Prettier config for `apps/web` and `packages/ui` sets
      `bracketSameLine: true`
- [ ] Both packages are reformatted in one commit, with no other changes

Verification:

- `pnpm lint`, `pnpm check`, `pnpm test`
- `just check`
