# Arc Lightning web app

The local UI for working with the workspace's indexed tasks, captures and
plan. `arcl serve` serves it on 127.0.0.1. Its views are in `src/routes`; the
components they share are in `packages/ui`.

To dogfood the app on claude.ai, `pnpm build:artifact` also builds it as an
artifact page in `build-artifact/index.html`. The artifact queues changes for
`arcl web apply`, board moves included. It hides the Task view's Edit file
editor, since the queue has no operation for a raw record edit.

## Tests

Run these from this directory, or with `pnpm --filter @arclightning/web`.

| Command                   | Runs                                                                  |
| ------------------------- | --------------------------------------------------------------------- |
| `pnpm test`               | Unit tests in Node and component tests in happy-dom                   |
| `pnpm test:browser`       | Route, layout and component tests in headless Chromium                |
| `pnpm test:ui`            | Both of the above in watch mode                                       |
| `pnpm test:e2e`           | Playwright workflows, fit and themes                                  |
| `pnpm test:visual`        | Screenshots of the views and the Work board, compared with baselines  |
| `pnpm test:visual:update` | Rewrites the baselines after an intended visual change                |
| `pnpm screenshots`        | Rewrites the README images in `docs/images/`: dark, rounded, 2x       |

The browser tests need Chromium. Set `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH` to
use one already installed. CI runs `just check`, which runs `pnpm test` and
leaves the rest out.

`test:e2e` runs a keyboard-only workflow against `arcl serve` on a scratch
project and checks each result with the CLI, and opens the artifact page
against a stand-in database recorded with `arcl web export`. The other
Playwright tests answer the API from `e2e/fixtures.json` through
`vite preview`. Set `WEB_E2E_PORT` when port 4174 is taken, and `ARCL_BIN` or
`CARGO_TARGET_DIR` to test another `arcl`.

Screenshot baselines are not committed. The first `test:visual` run writes
them to `e2e/*-snapshots` and fails each test as missing a baseline; later
runs compare against them. Review the new screenshots after
`test:visual:update`.

## The fast loop

For component and layout work, keep `pnpm test:ui` running here or in
`packages/ui`. Vitest keeps Chromium open and reruns only the tests a saved
file touches. A check that needs no server belongs in a browser test,
`src/**/*.svelte.test.ts`, which mounts a route with a fake data source (see
`src/lib/__tests__/`). Keep Playwright for what needs the built app: the
shell's routing, `arcl serve`, the artifact host and screenshots.

Run `pnpm test:e2e` before pushing. It builds first with
`scripts/e2e-build.mjs`, which:

- skips `build/` or `build-artifact/` when the files they are built from have
  not changed since their last successful build and its output, including
  what `vite preview` serves from `.svelte-kit/output`, is still there, so a
  change to tests alone goes straight to Playwright
- builds the out-of-date ones at the same time
- runs `cargo build -p arcl-cli`, which embeds `build/index.html`

Set `ARCL_REBUILD=1` to build both regardless. `test:visual` and
`screenshots` build `build/` the same way.
