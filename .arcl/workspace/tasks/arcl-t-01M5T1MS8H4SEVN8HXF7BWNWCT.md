+++
id = "arcl-t-01M5T1MS8H4SEVN8HXF7BWNWCT"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Declare TypeScript data shapes with type"
status = "pending"
priority = "normal"
position = 9
+++

Declare TypeScript data shapes with `type` instead of `interface` in
`apps/web` and `packages/ui`, and keep `interface` for contracts a class
implements, such as `ArtifactDb`, which `FakeDb` implements in the artifact
tests. Today the two packages hold about 130 `interface` declarations, most
of them API envelopes, records and component props.

Scope:

- `interface` to `type` for API envelopes, records, props and other data
  shapes, in `.ts` files and `.svelte` scripts
- the rule in the conventions list in `AGENTS.md`: data shapes use `type`,
  and `interface` is kept for contracts that classes implement

An option, not part of the criteria: `@typescript-eslint/consistent-type-definitions`
set to `type` would enforce the rule, with an inline disable on each class
contract. The packages have no ESLint setup today, so it needs one first.

Acceptance criteria:

- [ ] Every remaining `interface` in `apps/web` and `packages/ui` is one a
      class implements
- [ ] `AGENTS.md` states the rule
- [ ] No behaviour changes; type checks and tests pass unchanged

Verification:

- `grep -rn 'interface ' apps/web/src packages/ui/src` lists only class
  contracts
- `pnpm check`, `pnpm test`
- `just check`
