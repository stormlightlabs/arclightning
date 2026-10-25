+++
id = "arcl-t-01M5T1MS640ZDRP6XTZD3MEMK6"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Remove the barrel files in apps/web"
status = "pending"
priority = "normal"
position = 8
+++

Remove the barrel files in `apps/web` and import from the source modules
directly, so a reader follows an import to the code it names in one step.

Each feature folder under `apps/web/src/lib` (`capture`, `commands`, `inbox`,
`planning`, `sync`, `task` and `work`) has an `index.ts` that only re-exports
its siblings. `data/index.ts` re-exports `artifact`, `latest`, `serve` and
`types`, and also holds `dataSource`, `setDataSource` and `connectArtifact`.

`packages/ui/src/lib/index.ts` stays. It is the package's `.` entry in
`package.json` `exports`, which must name one module, and `apps/web` imports
`@arclightning/ui` through it. Replacing it with a subpath export per
component would widen the package's public surface for no reader's gain.

Scope:

- delete the re-export-only `index.ts` files in `apps/web/src/lib`
- move the code in `data/index.ts` to `data/source.ts` and drop its
  re-exports
- point every import at the module that defines the name

Acceptance criteria:

- [ ] No `index.ts` or `index.js` remains under `apps/web/src`
- [ ] `packages/ui/src/lib/index.ts` is the only barrel left, as the
      package entry
- [ ] Every `$lib` import names a module that defines what it imports

Verification:

- `find apps/web/src -name 'index.[jt]s'` prints nothing
- `pnpm check`, `pnpm test`
- `pnpm --filter @arclightning/web build` and `build:artifact`
- `just check`
