+++
id = "arcl-ph-01M5NH1M4357EDSS5J99NA8YY0"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 3: Repository-native collaboration"
status = "completed"
position = 3
+++

Adapt the existing workspace work to the new record types and finish safe
bidirectional synchronization.

Deliverables:

- the versioned `.arcl/workspace` format
- deterministic import and export for captures, specs, plans, tasks, and notes
- automatic reconciliation for one-sided changes
- record-level status and diagnostics
- explicit recovery for divergent database and file changes
- branch, detached HEAD, unborn HEAD, and unresolved merge handling without Git
  mutation
- this repository tracking its own roadmap and tasks in its committed workspace

Exit criterion: two branches can change different records and merge normally,
while Arc Lightning stops and explains true divergence before overwriting data
