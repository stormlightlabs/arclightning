# Arc Lightning roadmap

[SPEC.md](SPEC.md) defines the product. This roadmap orders the work needed to
reach it. [TODO.md](TODO.md) contains the current implementation tasks and their
acceptance criteria.

## Current foundation

Arc Lightning already has a working Rust CLI and SQLite model for ideas,
releases, spec-backed epics, milestones, tasks, dependencies, lifecycle state,
ready-work queries, task context, handoffs, and evidence. Snapshot encoding,
export, and import are also implemented.

This foundation is retained. Automatic snapshot reconciliation, conflict
recovery, full plan application, MCP, desktop, and the new product model are
unfinished.

## Milestone 1: Connected planning model

Move directly from the current tracker hierarchy to the project model in
[SPEC.md](SPEC.md). Each new record type includes the migration needed for
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

## Milestone 2: Shared Rust application

Extract the connected planning operations into reusable crates once their
responsibilities are established by working behavior.

Deliverables:

- core domain and application operations independent of delivery interfaces
- separate SQLite and repository-file responsibilities
- a CLI adapter that preserves the `arcl` binary
- removal of duplicate domain paths after migration

Exit criterion: the CLI passes through the shared Rust application layer, and
new adapters can use the same operations without invoking the CLI

## Milestone 3: Repository-native collaboration

Adapt the existing snapshot work to the new record types and finish safe
bidirectional synchronization.

Deliverables:

- the versioned `.arcl/workspace` format
- deterministic import and export for captures, specs, plans, tasks, and notes
- automatic reconciliation for one-sided changes
- record-level status and diagnostics
- explicit recovery for divergent database and file changes
- branch, detached HEAD, unborn HEAD, and unresolved merge handling without Git
  mutation

Exit criterion: two branches can change different records and merge normally,
while Arc Lightning stops and explains true divergence before overwriting data

## Milestone 4: Agent interface

Make the Rust application operations available directly to agents.

Deliverables:

- an MCP server with typed tools for capture, planning, task execution, notes,
  readiness, and context
- stable schemas and error responses
- an Arc Lightning skill that teaches agents the intended planning and handoff
  workflow without duplicating product rules

Exit criterion: an agent can inspect project context, claim ready work, leave a
handoff, and complete a task without parsing human CLI output

## Milestone 5: Shared interface system

Create the Svelte UI package before building the graphical product surfaces.

Deliverables:

- a pnpm workspace containing `apps/desktop`, `apps/website`, and the shared UI
  package
- the existing root website moved to `apps/website` without changing its
  behavior
- semantic light and dark theme tokens using navy/sky blue with
  gold/amber/yellow accents
- accessible controls, navigation, record summaries, status treatments, and
  Markdown presentation
- Playwright-backed Vitest browser tests for shared Svelte behavior
- Playwright screenshot baselines and a development surface for reviewing
  component states

Exit criterion: the website and a desktop test shell consume the same packaged
components without importing application-specific adapters

## Milestone 6: Desktop planning and execution

Build the Tauri app over the Rust application layer and shared UI package.

Deliverables:

- project opening and switching
- Inbox, Work, Planning, and Task views
- CodeMirror 6 Markdown editing and structured metadata controls
- readiness explanations, handoff, and completion flows
- keyboard, responsive, theme, and failure-state coverage
- Playwright workflow and screenshot coverage for the four primary views

Exit criterion: a developer can complete the primary planning and execution
workflow in the desktop app and observe the same results through the CLI

## Milestone 7: Website and documentation

Replace the current site with the shared visual system and the Stormlight
documentation house style.

Deliverables:

- a product landing page built with the shared UI package
- migrated documentation in a house-style shell implemented in `apps/website`
- navy/sky blue surfaces with gold/amber/yellow accents
- search, syntax highlighting, copy controls, metadata, sitemap, `llms.txt`, and
  static deployment output
- redirects or preserved URLs for useful existing documentation paths
- Playwright workflow and screenshot coverage for the landing and documentation
  layouts

Exit criterion: the static site builds cleanly, passes accessibility checks,
and documents the desktop, CLI, MCP, and repository-native workflows

## Milestone 8: Integrated release

Verify the complete system as one product and close only defects that block the
specified workflows.

Deliverables:

- end-to-end coverage across CLI, desktop, MCP, and repository-native files
- reviewed Playwright screenshot baselines for stable shared UI, desktop, and
  website states
- consistent errors and output across adapters
- packaging and installation documentation
- a release checklist tied to the success criteria in [SPEC.md](SPEC.md)

Exit criterion: every success criterion has automated evidence or a recorded
human review, and a new user can install Arc Lightning and complete the primary
workflow from the documentation
