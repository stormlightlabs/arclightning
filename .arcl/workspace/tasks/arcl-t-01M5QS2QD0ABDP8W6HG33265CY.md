+++
id = "arcl-t-01M5QS2QD0ABDP8W6HG33265CY"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.2: Build the Work list"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5QS2Q9VBVA27RW5M3V30HKF"]
+++

Build the Work view: the list a person opens to pick the next task, laid out
after Linear's issue list (Decision: Web app stack and look).

Scope:

- `apps/web/src/routes/` for the Work route
- list, group header and row components in `packages/ui/src/lib/components/`,
  reusing `Readiness.svelte`, `StatusBadge.svelte` and `PriorityBadge.svelte`
- unit tests beside the components and in `apps/web`

Acceptance criteria:

- [ ] Work lists tasks grouped by milestone, then by status, in the CLI's
      deterministic order
- [ ] Each row shows a readiness glyph, with the reason on hover and focus
- [ ] `j` and `k` move the selection, `enter` opens the task, and focus stays
      visible
- [ ] Filters narrow the list by status, readiness, priority and milestone,
      and the URL keeps them
- [ ] An empty or filtered-out list says so with `EmptyState`

Verification:

- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`: grouping, ordering, filters, and key
  handling
- `just check`

Out of scope:

- The Task view behind `enter` (T18.3)
- The command menu (T18.5)
