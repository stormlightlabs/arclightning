+++
id = "arcl-t-01M5S4N8CBY1JSE73MV0NN5BW4"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T24: Format apps/web and packages/ui with Prettier"
status = "completed"
priority = "normal"
position = 3
evidence = "Delivered by #55: apps/web and packages/ui have a Prettier config with prettier-plugin-svelte and `format`/`lint` scripts; the root `pnpm lint` runs in `just check`. Collapsing a `<style>` rule in either package makes `pnpm lint` exit 1. Verified with web build, build:artifact, cargo fmt/clippy/test, pnpm lint/check/test, test:e2e (42 passed) and arcl check."
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

- [x] Each CSS rule in a `.svelte` `<style>` block sits on its own lines,
      one declaration per line
- [x] `pnpm format` writes and `pnpm lint` checks `apps/web` and
      `packages/ui`
- [x] `just check` fails on an unformatted file in either package

Verification:

- Collapse a rule in a `<style>` block, and `just check` fails
- `pnpm lint`, `pnpm check`, `pnpm test`
- `just check`

Out of scope:

- `bracketSameLine` (T25)
