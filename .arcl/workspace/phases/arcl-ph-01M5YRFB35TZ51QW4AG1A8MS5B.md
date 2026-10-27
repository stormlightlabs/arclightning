+++
id = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 12: Agent workflow"
status = "open"
position = 12
+++

Make arcl easier for coding agents to pick up work, record what happened and
read exactly the data they need, with ideas taken from beads, beans and
chainlink.

Deliverables:

- claiming a task, and starting the next ready task in one step
- an agent primer and per-agent setup
- typed notes on a task while it is in progress
- gates that wait on pull requests, CI runs or dates
- capture provenance, a roadmap page and compaction of old work
- GraphQL queries over records
- a guard against databases written by a newer arcl

Exit criterion: two agents can work the same repository without taking the
same task, a new session gets its context from `arcl prime`, and an agent can
read only the fields it needs.
