+++
id = "arcl-t-01M5S4N8CBY1JSE73MV0NN5BW4"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T24: Format apps/web and packages/ui with Prettier"
status = "pending"
priority = "normal"
position = 3
+++

Format the Svelte and TypeScript in `apps/web` and `packages/ui` with
Prettier, so `<style>` blocks stop collapsing each rule onto one line. Only
`apps/website` has a Prettier config and a `format` script today, so nothing
formats or checks the other two packages.

Scope:

- a Prettier config with `prettier-plugin-svelte` that `apps/web` and
  `packages/ui` use, and an ignore file for their build output
- `format` and `lint` scripts in both packages
- one formatting commit across both packages
- the lint step in `just check`

Acceptance criteria:

- [ ] Each CSS rule in a `.svelte` `<style>` block sits on its own lines,
      one declaration per line
- [ ] `pnpm format` writes and `pnpm lint` checks `apps/web` and
      `packages/ui`
- [ ] `just check` fails on an unformatted file in either package

Verification:

- Collapse a rule in a `<style>` block, and `just check` fails
- `pnpm lint`, `pnpm check`, `pnpm test`
- `just check`

Out of scope:

- `bracketSameLine` (T25)
