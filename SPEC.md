# Arc Lightning product specification

Status: Direction approved; implementation pending

## Objective

Arc Lightning is a local-first project planning and execution system for
developers and software agents. It connects specifications, implementation
plans, executable tasks, and working notes in one project model.

People and agents use the same model through a desktop app, command-line
interface (CLI), Model Context Protocol (MCP) server, and optional files stored
with the project. The SQLite database is the operational store. Markdown is the readable form
for long-form content.

## Product principles

- Local use works without an account, hosted service, or network connection
- Git integration is optional. A project can use SQLite alone or keep a
  repository-native projection in sync with the database
- The CLI, desktop app, and MCP server are peers over the same Rust application
  operations
- Planning documents and execution state belong to one connected model
- Markdown bodies are readable and editable outside Arc Lightning
- Readiness, blockers, lifecycle transitions, handoffs, and completion evidence
  are deterministic and queryable
- Simple work does not require an empty plan or phase for ceremony

## Users and use cases

The primary user is a developer who plans work in Markdown, works from a
terminal or desktop app, and may pass tasks between people and coding agents.

A developer can:

- create or open a project without requiring Git
- capture an unstructured thought and later turn it into a spec, task, or note
- write and edit specs, plans, notes, and task descriptions as Markdown
- break a spec into one or more plans, optional phases, tasks, and subtasks
- create a task directly under a project or spec when a plan adds no value
- model blockers and ask why work is ready or blocked
- resume work from the latest handoff and inspect completion evidence
- use the same project from the desktop app and CLI
- give an agent a focused context packet or use Arc Lightning through MCP
- opt into reviewable project files that can be stored and merged with Git

## Success criteria

The product direction is realized when:

- a non-Git directory can host a complete Arc Lightning project
- schema changes preserve descriptions, relationships, lifecycle state,
  handoffs, evidence, and stable identifiers when their type is still valid
- captures, specs, plans, optional phases, tasks, notes, and releases are
  persistent domain objects
- releases use a general membership relationship for specs, plans, tasks, and
  notes
- specs and plans own their Markdown content instead of pointing at documents
  that Arc Lightning cannot edit
- every supported operation uses the same Rust application layer from the CLI,
  desktop adapter, and MCP adapter
- flexible task placement preserves deterministic readiness and dependency
  rules
- repository-native mode round-trips the complete model and refuses to
  overwrite divergent database and file changes
- the Tauri app provides inbox, work, planning, and task views
- the Tauri app and website consume one shared UI package for design tokens and
  reusable Svelte components
- the documentation follows the Stormlight house style described below
- human output is concise and machine output is versioned and stable
- focused tests cover domain rules, storage migrations, adapters, UI behavior,
  and repository synchronization
- Playwright screenshot tests cover stable desktop, website, and shared UI
  states in light and dark themes

## Domain model

```text
Project
|-- Capture
|-- Spec
|   `-- Plan
|       |-- Phase (optional)
|       `-- Task
|-- Task
|-- Note
`-- Release
```

Relationships can cross this hierarchy where the model calls for them. All
linked records belong to the same project.

### Project

A project contains the planning and execution records for one body of work. It
may refer to a Git worktree, but Git is not part of the domain requirement.

The desktop app can open more than one project. Each project keeps its own
operational database and optional repository-native workspace.

### Capture

A capture replaces the current `Idea` term. It is an inbox record with a title,
Markdown body, status, and creation metadata.

A capture can be promoted to a spec, task, or note, or it can be discarded.
Promotion preserves provenance and is idempotent for the same target.

### Spec

A spec replaces the current `Epic` concept. It has a title, lifecycle state,
Markdown body, acceptance criteria, and links to related records.

Arc Lightning owns the spec content. Repository-native mode can project that
content to a Markdown file, but the file is a representation of the record
rather than an unrelated external document.

### Plan and phase

A plan is a persistent description of how a spec will be implemented. One spec
can have more than one plan.

A plan can contain tasks directly or group them into ordered phases. A phase is
the successor to the current milestone container. Phases are optional.

The plan check, diff, and apply behavior provides an import and automation
interface. Applying structured input creates or updates a
persistent plan and its tasks transactionally.

### Task

A task belongs to a project and can optionally belong to a spec, plan, phase,
or parent task. A plan or phase is not required. Parent and child tasks share
the same task type.

Tasks retain:

- status and priority
- ordered child tasks
- dependency edges and deterministic ready-work calculation
- a current Markdown handoff
- Markdown completion evidence

Readiness is calculated for actionable leaf tasks. A task is ready when it is
pending, its applicable ancestors and containers permit work, and every direct
blocker is complete. Notes never affect readiness.

### Note

A note has a title, Markdown body, and links to related records. Notes hold
research, decisions, meeting notes, implementation discoveries, references,
and debugging notes.

Notes are addressable records. They do not have task readiness or task
lifecycle behavior.

### Release

A release is an optional named collection of planned deliverables. Specs,
plans, tasks, and notes can be release members through a many-to-many
relationship. Membership does not implicitly include a record's descendants.
Captures cannot join a release until they are promoted, and phases belong to
their plan.

## Interfaces

### CLI

The `arcl` CLI is a primary interface. Its vocabulary uses `capture`,
`spec`, `plan`, `task`, and `note`. These commands replace the current `idea`,
`epic`, and `milestone` commands without compatibility aliases.

Read commands support stable JSON. Mutations use the same validation and
transactions as the desktop app and MCP server.

### Desktop app

The desktop app uses Tauri and a Svelte frontend. Tauri commands are thin
adapters over Rust application operations and do not duplicate domain rules.

The first complete desktop workflow has four views:

- Inbox captures and triages unplanned work
- Work shows backlog, ready, in-progress, and parked tasks
- Planning navigates specs, plans, phases, tasks, and notes
- Task shows description, ancestry, blockers, handoff, and evidence

The editor uses CodeMirror 6 for Markdown bodies alongside structured metadata.
Keyboard access, visible focus, reduced motion, and screen-reader names are
acceptance requirements rather than a later polish pass.

### MCP and agent context

The MCP server exposes the same application operations that make sense for an
agent. It returns typed records and errors instead of parsing CLI presentation
text.

`arcl context` is the focused task handoff for command-line agents. It
includes the task, relevant ancestors, spec and plan context, direct blockers,
latest handoff, and relevant evidence without returning the whole project.

### Repository-native workspace

Repository-native mode projects the database into one record per Markdown/TOML
file under a configurable workspace directory. The target layout is:

```text
.arcl/
|-- config.toml
|-- arcl.db                 # ignored
`-- workspace/              # optional tracked projection
    |-- manifest.toml
    |-- captures/
    |-- specs/
    |-- plans/
    |-- tasks/
    `-- notes/
```

Synchronization uses a three-state comparison: compare
the last common files, current database projection, and current workspace. Arc
Lightning imports or exports one-sided changes and stops on different changes
to both sides. It never resolves a conflict by silently choosing a side.

## Architecture and repository shape

The target repository separates the reusable Rust application from delivery
interfaces and keeps shared Svelte components in one package:

```text
arclightning/
|-- crates/
|   |-- arcl-core/          # domain rules and application operations
|   |-- arcl-store/         # SQLite and migrations
|   |-- arcl-repo/          # portable files and Git-aware reconciliation
|   |-- arcl-cli/
|   `-- arcl-mcp/
|-- apps/
|   |-- desktop/            # Tauri application
|   `-- website/            # product site and documentation
|-- packages/
|   `-- ui/                 # shared Svelte components and design tokens
|-- skills/
|   `-- arclightning/
`-- docs/
```

This layout is a target, not a requirement to move every file at once. Each
move must leave the affected interface buildable and tested.

The Rust core must not depend on Tauri, terminal presentation, MCP transport,
or frontend persistence. The shared UI package must not depend on Tauri APIs or
website-only routing. Each application supplies those adapters at its
composition root.

## Shared UI and visual direction

The Tauri app and website use the shared UI package for:

- semantic color, type, spacing, border, shadow, motion, and focus tokens
- buttons, icon buttons, fields, selectors, dialogs, sheets, menus, tabs,
  badges, empty states, and feedback
- navigation, record summaries, status and priority treatments, Markdown
  display, and common planning controls
- semantic icons exposed by component name rather than raw icon-library names

Application shells and route-specific compositions stay in their applications.
The package owns reusable presentation and interaction behavior, not storage,
routing, or Tauri commands.

The Arc Lightning palette uses navy and sky blue as its base, with gold, amber,
and yellow for emphasis. Semantic tokens must cover light and dark themes and
meet WCAG 2.2 AA contrast for text, controls, and focus indicators. Components
must use semantic roles such as canvas, surface, text, accent, warning, danger,
and focus instead of hard-coded colors.

## Documentation house style

The website documentation follows the established patterns in `../mire`,
`../dalil`, and `../inkfinite` while retaining Arc Lightning's own identity.
The implementation should reuse the pattern, not copy another product's brand.
The documentation shell is implemented directly in `apps/website`. It can use
shared UI primitives, but its content navigation, search, table of contents,
and documentation layout belong to the website.

Required characteristics are:

- a focused landing page with a direct product statement, primary action, and
  concrete product example
- a sticky header, grouped sidebar, article column, on-page table of contents,
  breadcrumbs, previous/next navigation, search, theme control, and GitHub link
- Markdown source with typed front matter and predictable navigation order
- copy-code and copy-Markdown controls, syntax highlighting, generated social
  metadata, sitemap, `llms.txt`, and a static build
- IBM Plex Sans for body copy, Google Sans for headings and interface labels,
  and Google Sans Code for code and compact metadata
- responsive navigation, a skip link, visible focus, reduced-motion support,
  readable line lengths, and light and dark themes
- Arc Lightning navy/sky surfaces and links with gold/amber/yellow accents for
  calls to action, current navigation, code emphasis, and selected states

The current Astro/Starlight site can be replaced when the new shell is ready.
Content migration must preserve useful URLs or add redirects.

## Current state and migration constraints

The current repository is a single Rust crate with a CLI, SQLite migrations,
domain and storage modules, snapshot codecs, snapshot import/export, and CLI
tests. The website uses Astro and Starlight at `website/`. The website work
moves it to `apps/website` as part of the pnpm workspace transition.

The implemented foundation includes ideas, releases, epics, milestones, tasks,
dependencies, lifecycle rules, readiness, context, handoffs, evidence, and the
first snapshot codec/import/export path. Automatic snapshot reconciliation,
conflict tooling, complete plan application, and release hardening are not
finished.

The migration must:

- preserve working behavior before changing names or storage shapes
- use expand, migrate, and contract steps when old and new records must coexist
- keep database migrations forward-only and test upgrades from representative
  databases
- define a versioned migration for existing snapshot files before changing the
  snapshot manifest
- avoid completing obsolete synchronization work against the old hierarchy
  when the same work can be completed against the new model

## Testing and verification

The stable behavior boundaries are the Rust application API for domain and
storage behavior, adapter-level tests for CLI and MCP, component tests for the
shared UI, and end-to-end tests for desktop and repository synchronization.

The frontend test stack is:

- Vitest for TypeScript logic, state helpers, validation, and SvelteKit server
  or SSR behavior
- Vitest browser projects using `@vitest/browser-playwright` and
  `vitest-browser-svelte` for Svelte components that depend on real browser
  events, focus, layout, visibility, or lifecycle
- Playwright Test for critical user workflows and screenshot comparisons

Browser tests use `page` locators and query by role, label, or visible text
before using test IDs. Component tests assert user-visible behavior rather than
Svelte implementation details. Server tests use real `Request` and `FormData`
objects where those interfaces apply.

Screenshot tests supplement behavioral assertions. They cover shared component
states, the four desktop views, the website landing page, and representative
documentation pages. The screenshot environment pins Chromium, viewport,
color scheme, fonts, fixture data, and motion preferences. Baseline changes
require review and are updated only when the visual change is intentional.

Desktop Playwright tests run the Svelte interface through a deterministic
browser harness backed by a test application adapter. Rust integration tests
cover the Tauri command boundary against the real application operations.

Current Rust checks:

```sh
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

After the pnpm workspace and applications exist, each frontend package provides
`check`, `test`, `test:browser`, and `build` scripts where they apply. Packages
with complete user workflows also provide `test:e2e` and `test:visual`
Playwright scripts. Human review covers keyboard use, responsive layouts, theme
contrast, Markdown editing, screenshot diffs, and conflict recovery copy.

## Working boundaries

Implementation may reuse existing helpers, move code behind clearer crate
boundaries, add tests at established behavior boundaries, and evolve internal
APIs as required by this spec.

Ask before adding a hosted service, changing the repository-native file format
without a migration, or making releases control readiness.

Never require Git for core project use, put frontend-specific behavior in the
Rust domain, put Tauri or website adapters in the shared UI package, silently
discard existing project data, or choose a side during repository divergence
without an explicit user command.
