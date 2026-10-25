+++
id = "arcl-t-01M5QS2QSJ1S7D1XG5Z6HAY1G3"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.6: Build the Inbox and Planning views"
status = "completed"
priority = "normal"
position = 6
blocked-by = ["arcl-t-01M5QS2QP9TNCV3A88EHX375D9"]
evidence = "Delivered by #PR: the Inbox lists open captures and promotes or discards them (1, 2, 3, Backspace); Planning shows milestones with progress and ready counts, opens their task trees, and creates specs and plans and applies plan documents. arcl serve adds GET /api/tree and POSTs for capture promote and discard, spec create, plan create and plan apply. Verified capture -> task and capture -> spec -> plan -> task in the browser against arcl show and arcl tree."
+++

Build the Inbox and Planning views, so captures become planned work in the
browser.

Scope:

- `apps/web/src/routes/` for the Inbox and Planning routes
- endpoints in `crates/arcl-serve` to promote a capture, create specs and
  plans, and apply a plan
- `PlanningTree.svelte` for the hierarchy

Acceptance criteria:

- [x] Inbox lists open captures and promotes one to a task, a spec, or a note
- [x] A capture becomes a spec, the spec a plan, and the applied plan tasks,
      all from the browser
- [x] Planning shows each milestone with its phase, progress and ready count,
      and opens the planning tree below it
- [x] The views follow the Work view's keyboard model

Verification:

- `cargo test --workspace --all-features serve`: promote, spec and plan
  create, and plan apply
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`
- Complete capture -> task and capture -> spec -> plan -> task in the browser
  and confirm the results with `arcl show` and `arcl tree`
- `just check`

Out of scope:

- Browser test coverage (T18.7)
