+++
id = "arcl-ph-01M5NH1M0T75BYF0YNKHZXMYY3"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 1: Connected planning model"
status = "completed"
position = 1
+++

Move directly from the current tracker hierarchy to the project model in
`SPEC.md`. Each new record type includes the migration needed for
existing data.

Deliverables:

- projects that work with or without a Git worktree
- captures that replace ideas
- owned specs that replace spec-linked epics
- persistent plans and optional phases that replace mandatory milestones
- tasks that can live under a project, spec, plan, phase, or task where valid
- linkable Markdown notes
- general release membership for specs, plans, tasks, and notes
- readiness, blockers, handoffs, evidence, and context for the flexible hierarchy
- CLI commands that replace the old nouns with the new vocabulary

Exit criterion: a migrated or new project can complete the capture -> spec ->
plan -> task workflow, while small work can move from capture directly to task
