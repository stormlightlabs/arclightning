+++
id = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 4: Blocking for automation"
status = "open"
position = 4
+++

Make blocking relationships strict and machine-readable so agents and
automations can rely on them.

Deliverables:

- blockers on a parent task also block its descendants
- `task start` from pending refuses work that is not ready unless the caller
  passes `--force`; a repeat start stays a no-op
- blocker edges cannot form a cycle or join a task to its own ancestor or
  descendant
- a cancelled blocker is reported as a decision to make, not a silent wait
- typed readiness reasons in JSON alongside the human text
- task completion reports the tasks it made ready

Exit criterion: an automation can pick ready work, learn why other work is
blocked, and chain from one completed task to the work it unblocked using JSON
alone
