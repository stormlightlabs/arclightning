+++
id = "arcl-t-01M5QXMQFGA7QPRKNWPCBVN21N"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "T20: Publish the web app as a claude.ai artifact"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NE5P6XHRR4Q84820YACQ07"]
evidence = "Delivered by #48: `pnpm --filter @arclightning/web build:artifact` builds the views into one `index.html` over an artifact backend that reads the `arcl serve` envelopes from the database and queues edits. `arcl web export` and `arcl web apply` run the same Reads and Mutations as `arcl serve`. Verified by cargo, vitest and Playwright tests against a stand-in database recorded with `arcl web export`. Not verified yet: publishing on claude.ai, syncing this workspace and triaging a capture there."
+++

Publish the web app as a claude.ai artifact, so a person can browse and
triage a project's plan from claude.ai without running `arcl serve`. The
local app keeps the `arcl serve` backend; the artifact build swaps it for the
artifact's database.

Blocked by: T18

The artifact can't reach `arcl serve` or the repository, so an agent keeps
its database in step with the workspace, the way the Fabrial Ledger does
today. Edits made on the page are queued in the database, and an agent
applies them with `arcl` and syncs the result back.

Scope:

- a second build target in `apps/web` that emits one `index.html` for the
  artifact
- an artifact backend for the data module T18.2 adds
- a sync command or script that writes the workspace's records into the
  artifact's database and applies queued edits through `arcl`
- docs for publishing and syncing

Acceptance criteria:

- [x] The artifact build reuses every T18 view without forking them
- [x] Work, Task, Inbox and Planning render from the artifact's database
- [x] An edit made on the page is applied through `arcl` and shows in the
      workspace after a sync, with the CLI's validation and errors
- [x] A sync after a workspace change updates only the records that changed

Verification:

- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`: the artifact backend against
  recorded database fixtures
- Publish the build, sync this repository's workspace, triage a capture on
  the page, and confirm the result through the CLI

Out of scope:

- A hosted MCP server or any other backend that reaches the repository
  directly
