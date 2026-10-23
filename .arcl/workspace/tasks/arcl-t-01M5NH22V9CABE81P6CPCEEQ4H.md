+++
id = "arcl-t-01M5NH22V9CABE81P6CPCEEQ4H"
phase = "arcl-ph-01M5NH1M0T75BYF0YNKHZXMYY3"
title = "T02: Add the connected planning records"
status = "completed"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M5NH1M7KC96F91H2KMACN5AX"]
evidence = """
Delivered by commits to main before pull requests:

- 64ebaa7 (feat: connected planning schema and storage layer)
- 314d2c2 (chore: consolidate baseline)"""
+++

Add captures, owned specs, persistent plans, optional phases, flexible tasks,
and notes to the existing application.

Blocked by: T01

Acceptance criteria:

- [x] Add typed records and baseline schema definitions for each new entity and
      relationship in `SPEC.md`
- [x] Store Markdown bodies for specs, plans, notes, and tasks in the operational
      database
- [x] Allow a task under a project, spec, plan, phase, or parent task and reject
      contradictory or cross-project ancestry
- [x] Add many-to-many release membership for specs, plans, tasks, and notes
      without implicitly including descendants
- [x] Include ideas, epics, milestones, tasks, and links in the baseline schema
- [x] Preserve identifiers across records that use the same entity type

Verification:

- `cargo test --workspace --all-features domain`
- `cargo test --workspace --all-features migrations`
- Create a project and confirm its records and relationships are available
