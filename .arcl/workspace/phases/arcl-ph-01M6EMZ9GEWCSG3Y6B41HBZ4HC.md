+++
id = "arcl-ph-01M6EMZ9GEWCSG3Y6B41HBZ4HC"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 14: Tracker integrations"
status = "open"
position = 14
+++

Opt-in mirrors of arcl projects in external trackers, so a team or a phone
can follow the work without running arcl. arcl stays the source of truth for
planning; a tracker gets a push-mostly copy. Linear is the first adapter.

Deliverables:

- tracker settings per project in `.arcl/config.toml`
- tracker IDs stored on workspace records, so the mapping survives a clone
- `arcl linear push`: phases to milestones, tasks to issues, subtasks to
  sub-issues, blockers to relations, idempotent
- edits made in the tracker reported on the next push, never overwritten

Exit criterion: a project with Linear settings pushes its plan to a Linear
project twice with no changes the second time, and a status changed in Linear
shows up as a report instead of being lost.
