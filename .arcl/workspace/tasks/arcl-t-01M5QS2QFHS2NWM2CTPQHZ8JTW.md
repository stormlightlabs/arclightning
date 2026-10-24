+++
id = "arcl-t-01M5QS2QFHS2NWM2CTPQHZ8JTW"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.3: Build the Task view"
status = "completed"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5QS2QD0ABDP8W6HG33265CY"]
evidence = "Delivered by #40: the Task view in apps/web, DataSource.task in apps/web/src/lib/data, and PropertiesPanel and MarkdownEditor (CodeMirror 6) in packages/ui. Verified with pnpm --filter @arclightning/web check and test, the packages/ui browser tests, a headless Chromium run against arcl serve on this repository, and the just check steps."
+++

Build the Task view: everything needed to decide whether a task can be worked
and to pick it up.

Scope:

- `apps/web/src/routes/` for the Task route
- a properties panel and a Markdown editor component in
  `packages/ui/src/lib/components/`, the editor on CodeMirror 6 with its
  Markdown language
- unit tests beside the components and in `apps/web`

Acceptance criteria:

- [x] The Task view shows readiness reasons, direct blockers, and blockers
      inherited from ancestors, each linking to its record
- [x] It shows the latest handoff and the evidence recorded on the task
- [x] A properties panel shows status, priority, parent, spec, plan and phase,
      and collapses and expands from the keyboard
- [x] The body renders as Markdown and opens in CodeMirror 6 read-only
- [x] `escape` returns to the Work list with the same row selected

Verification:

- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`: reasons, both blocker kinds,
  handoff, evidence, and the panel's state
- `just check`

Out of scope:

- Editing the body and changing status (T18.4)
