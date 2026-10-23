+++
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
spec = "arcl-s-01M5NE5NZPWXPVWMZA5CJBD6MY"
title = "Roadmap"
status = "open"
+++

`SPEC.md` defines the product. This plan orders the work needed to reach it.

Each phase is one milestone, positioned in milestone order; `arcl tree
arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC` lists them that way, with their tasks.
`--plain` adds each record's status. A phase body holds the milestone's
deliverables and exit criterion. Its tasks carry their acceptance criteria and
verification, and completed tasks carry evidence naming the commits and pull
requests that delivered them. Task numbers are stable labels, not an order:
blocker edges set the order, and `arcl ready` lists the work that can start.

The desktop app, milestone 10, follows the first release.

Project decisions are notes titled `Decision: <name>`, linked to this plan
and, where one applies, to a phase. `arcl note list` shows them.

## Current foundation

Arc Lightning has a working Rust CLI over a shared application layer and
SQLite. It covers projects with or without Git, captures, specs, plans, optional
phases, flexible tasks, notes, releases, dependencies, lifecycle state,
ready-work queries, task context, handoffs, and evidence. The shared Svelte UI
package exists. The repository-native workspace syncs around every command,
reports status, and stops for explicit recovery on divergence or unmerged Git
files. This repository tracks its roadmap and tasks in `.arcl/workspace`.
