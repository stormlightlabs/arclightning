# Arc Lightning implementation tasks

Source: [SPEC.md](SPEC.md)

Sequence: [ROADMAP.md](ROADMAP.md)

Tasks are listed in dependency order. Completed v1 foundations are summarized
in the roadmap and are not repeated here.

## T01: Support projects outside Git worktrees

Allow Arc Lightning to initialize and discover a project in an ordinary
directory while preserving optional Git-aware behavior.

Blocked by: None - can start immediately

Acceptance criteria:

- [ ] `arcl init` succeeds in a non-Git directory and creates a local project
- [ ] Commands discover the nearest Arc Lightning project from descendant
      directories without relying on Git discovery
- [ ] Git-backed projects still report repository state when repository-native
      mode uses it
- [ ] Core domain and storage operations do not require a Git repository

Verification:

- `cargo test --workspace --all-features project_discovery`
- Complete init, idea, task, and ready operations in disposable Git and non-Git
  directories

## T02: Add the connected planning records

Add captures, owned specs, persistent plans, optional phases, flexible tasks,
and notes to the existing application.

Blocked by: T01

Acceptance criteria:

- [ ] Add typed records and forward-only migrations for each new entity and
      relationship in `SPEC.md`
- [ ] Store Markdown bodies for specs, plans, notes, and tasks in the operational
      database
- [ ] Allow a task under a project, spec, plan, phase, or parent task and reject
      contradictory or cross-project ancestry
- [ ] Add many-to-many release membership for specs, plans, tasks, and notes
      without implicitly including descendants
- [ ] Migrate current ideas, epics, milestones, tasks, and links as part of the
      new schema migration
- [ ] Preserve current identifiers when the entity type remains valid and store
      explicit mappings where it changes

Verification:

- `cargo test --workspace --all-features domain`
- `cargo test --workspace --all-features migrations`
- Create a current-format project, upgrade it, and confirm its records and
  relationships remain available

## T03: Implement capture promotion and owned planning content

Let users create and edit the new records and move a capture into a spec, task,
or note without losing provenance.

Blocked by: T02

Acceptance criteria:

- [ ] Create, show, update, list, and transition captures, specs, plans, phases,
      and notes through application operations
- [ ] Promote a capture transactionally to a spec, task, or note
- [ ] Make repeated promotion to the same target idempotent and reject an
      ambiguous second target
- [ ] Check, diff, and apply structured plan input against a persistent plan
      without duplicating tasks on repeated application
- [ ] Return validation errors without partial writes

Verification:

- `cargo test --workspace --all-features capture`
- `cargo test --workspace --all-features plan`
- Exercise create, edit, promotion, plan diff, and repeated plan apply through
  the application API

## T04: Adapt task execution to flexible ancestry

Preserve lifecycle, dependencies, readiness, context, handoff, and evidence
when phases and plans are optional.

Blocked by: T02, T03

Acceptance criteria:

- [ ] Compute ready leaf tasks for every valid placement without assuming a
      milestone exists
- [ ] Reject dependency cycles, parent cycles, cross-project links, and invalid
      container states before writing
- [ ] Explain readiness and blocking in terms of the actual ancestry present
- [ ] Include relevant spec, plan, phase, blocker, handoff, and evidence content
      in task context without returning unrelated records
- [ ] Preserve atomic handoff, parking, completion, and evidence behavior

Verification:

- `cargo test --workspace --all-features ready`
- `cargo test --workspace --all-features context`
- Cover project-, spec-, plan-, phase-, and parent-task placement in tests

## T05: Move the CLI to the new vocabulary

Expose the connected planning model through `arcl` and replace the old command
nouns.

Blocked by: T03, T04

Acceptance criteria:

- [ ] Add coherent `capture`, `spec`, `plan`, `task`, and `note` command groups
- [ ] Support stable JSON for every new read operation and structured errors for
      expected failures
- [ ] Remove the `idea`, `epic`, and `milestone` command groups when their
      replacements ship and do not add compatibility aliases
- [ ] Update help examples so a user can complete capture -> spec -> plan ->
      task and capture -> task workflows
- [ ] Keep primary results on stdout and diagnostics on stderr without ANSI
      escapes in machine output

Verification:

- `cargo test --workspace --all-features cli`
- Run both primary workflows using human and JSON output

## T06: Extract the shared Rust application

Move the working connected planning operations behind core, store, repository,
and CLI crate boundaries without changing their behavior.

Blocked by: T04, T05

Acceptance criteria:

- [ ] Create workspace crates for domain and application operations, SQLite,
      repository files, and the CLI
- [ ] Keep domain and application crates independent of Clap, terminal output,
      Tauri, and MCP transports
- [ ] Preserve the binary name, commands, exit behavior, and database
      compatibility
- [ ] Move tests to the highest stable crate or CLI boundary without duplicating
      coverage

Verification:

- `cargo check --workspace --all-targets --all-features`
- `cargo test --workspace --all-features`
- Run the capture, planning, ready-work, context, and handoff CLI workflows

## T07: Remove the superseded internal model

Remove old storage and internal type paths after production callers use the
connected planning model.

Blocked by: T05, T06

Acceptance criteria:

- [ ] Remove legacy tables or fields only through a forward migration with
      upgrade coverage in this task
- [ ] Remove duplicate domain paths after all production callers use the new
      records
- [ ] Confirm upgraded projects retain all supported Markdown, relationships,
      handoffs, and evidence

Verification:

- `cargo test --workspace --all-features migrations`
- `cargo test --workspace --all-features`

## T08: Migrate repository-native files to the new model

Adapt the existing codec, import, and export work to the versioned workspace
layout in `SPEC.md`.

Blocked by: T06, T07

Acceptance criteria:

- [ ] Define and document a new manifest version and record encoding for
      captures, specs, plans, tasks, and notes
- [ ] Import current snapshot records through an explicit migration path
- [ ] Render one deterministic file per record and preserve Markdown bodies
      through parse-render-parse round trips
- [ ] Validate the complete candidate graph before mutating the database
- [ ] Export changed records atomically and leave unchanged files byte-stable

Verification:

- `cargo test --workspace --all-features snapshot::codec`
- `cargo test --workspace --all-features snapshot::import`
- `cargo test --workspace --all-features snapshot::export`

## T09: Finish automatic reconciliation and conflict recovery

Synchronize one-sided database or workspace changes and provide explicit tools
for true divergence.

Blocked by: T08

Acceptance criteria:

- [ ] Implement every state in the base/database/workspace comparison from
      `SPEC.md`
- [ ] Reconcile before ordinary reads and writes, including recovery after an
      interrupted post-commit export
- [ ] Refuse automatic writes when the database and workspace changed
      differently from their common base
- [ ] Report record-level status, unmerged paths, conflict markers, and Git path
      state where Git exists
- [ ] Require an explicit side for destructive recovery and never mutate Git
      refs, the index, remotes, or configuration

Verification:

- `cargo test --workspace --all-features snapshot::sync`
- `cargo test --workspace --all-features snapshot::reconcile`
- Exercise external edits, a branch switch, an unresolved merge, dual-side
  divergence, and interrupted export in disposable projects

## T10: Add the MCP server and Arc Lightning skill

Expose agent operations through typed MCP tools and document the intended
planning, context, handoff, and evidence workflow in a project skill.

Blocked by: T06

Acceptance criteria:

- [ ] Add MCP tools for record reads and mutations, ready work, readiness
      explanation, context, handoff, and completion
- [ ] Reuse application operations and errors without invoking or scraping the
      CLI
- [ ] Publish versioned input and output schemas with bounded list pagination
- [ ] Add an Arc Lightning skill that refers to product commands and MCP tools
      without restating domain rules
- [ ] Cover tool authorization boundaries and invalid-transition errors

Verification:

- `cargo test --workspace --all-features -p arcl-mcp`
- Run one capture-to-completion workflow through MCP

## T11: Create the pnpm workspace and shared UI package

Build the reusable Svelte component and token package consumed by the desktop
app and website.

Blocked by: None - can start immediately

Acceptance criteria:

- [ ] Move the existing `website` directory to `apps/website` and update root
      scripts, package references, and development commands without redesigning
      the site in this task
- [ ] Configure a pnpm workspace for `packages/ui`, `apps/desktop`, and
      `apps/website` without duplicating frontend dependencies
- [ ] Define semantic light and dark tokens using navy/sky blue with
      gold/amber/yellow accents
- [ ] Add accessible primitives for buttons, fields, menus, dialogs, tabs,
      status, priority, feedback, and Markdown display
- [ ] Add planning components needed by both consumers, with application data
      supplied through props and events
- [ ] Keep Tauri APIs, routing, persistence, and website content out of the
      package
- [ ] Provide a component review surface and tests for state, keyboard use,
      focus, theme, and reduced motion
- [ ] Configure Vitest unit and browser projects with
      `@vitest/browser-playwright`, Chromium, and `vitest-browser-svelte`
- [ ] Use `page` locators and accessible role, label, or text queries in browser
      component tests
- [ ] Add Playwright screenshot tests for stable shared component states in
      light and dark themes
- [ ] Pin viewport, color scheme, fonts, fixture data, and motion preferences for
      screenshot runs

Verification:

- `pnpm --filter @arclightning/ui check`
- `pnpm --filter @arclightning/ui test`
- `pnpm --filter @arclightning/ui test:browser`
- `pnpm --filter @arclightning/ui test:visual`
- `pnpm --filter @arclightning/ui build`
- Review light, dark, narrow, hover, focus, disabled, loading, empty, and error
  states

## T12: Build the desktop shell and project inbox

Create the Tauri application, connect it to Rust application operations, and
deliver project selection plus capture triage.

Blocked by: T01, T03, T06, T11

Acceptance criteria:

- [ ] Open, create, remember, and switch between Arc Lightning projects
- [ ] Use thin Tauri commands over the Rust application layer
- [ ] List, create, edit, promote, and discard captures in the Inbox view
- [ ] Show actionable validation and storage errors without exposing internal
      traces
- [ ] Match CLI results for the same project mutations
- [ ] Provide a deterministic browser test adapter for desktop Playwright tests
      without duplicating domain behavior
- [ ] Cover the thin Tauri command boundary with Rust integration tests

Verification:

- `cargo test --workspace --all-features -p arcl-desktop`
- `pnpm --filter @arclightning/desktop check`
- `pnpm --filter @arclightning/desktop test`
- `pnpm --filter @arclightning/desktop test:browser`
- `pnpm --filter @arclightning/desktop test:e2e`
- Complete capture -> task and capture -> spec flows through Playwright

## T13: Add desktop work, planning, and task views

Complete the graphical planning and execution workflow over the shared project
model.

Blocked by: T04, T12

Acceptance criteria:

- [ ] Show backlog, ready, in-progress, and parked work with readiness reasons
- [ ] Navigate and edit specs, plans, optional phases, tasks, and notes in the
      Planning view
- [ ] Use CodeMirror 6 for Markdown editing with source-mode keyboard and
      accessibility behavior covered by component tests
- [ ] Show task ancestry, blockers, Markdown description, handoff, and evidence
      on the Task view
- [ ] Support start, park, handoff, unpark, complete, and cancel transitions with
      the same validation as the CLI
- [ ] Preserve selection and unsaved-edit safety during navigation and external
      project changes
- [ ] Support keyboard-only use, narrow windows, light and dark themes, and
      reduced motion
- [ ] Add Playwright screenshot coverage for Inbox, Work, Planning, and Task at
      fixed desktop and narrow viewports in light and dark themes

Verification:

- `pnpm --filter @arclightning/desktop check`
- `pnpm --filter @arclightning/desktop test`
- `pnpm --filter @arclightning/desktop test:browser`
- `pnpm --filter @arclightning/desktop test:e2e`
- `pnpm --filter @arclightning/desktop test:visual`
- Run Playwright flows for planning, blockers, handoff, and completion

## T14: Rebuild the website and documentation shell

Use the shared UI package for the product website and implement Arc Lightning's
version of the Stormlight documentation house style.

Blocked by: T11

Acceptance criteria:

- [ ] Replace the current Astro/Starlight presentation only after the new static
      site builds and all retained content has a destination
- [ ] Build the landing page with shared UI components and a concrete Arc
      Lightning workflow example
- [ ] Add the sticky header, grouped sidebar, breadcrumbs, article layout,
      on-page table of contents, previous/next links, responsive menu, search,
      and theme control used by the sibling documentation sites
- [ ] Implement the documentation shell directly in `apps/website`, using the
      shared UI package only for reusable primitives
- [ ] Use IBM Plex Sans, Google Sans, and Google Sans Code with Arc Lightning's
      navy/sky and gold/amber/yellow theme
- [ ] Add copy-code, copy-Markdown, syntax highlighting, social metadata,
      sitemap, `llms.txt`, and static output
- [ ] Preserve useful documentation URLs or add redirects and update content for
      the new product vocabulary
- [ ] Pass keyboard, contrast, responsive, reduced-motion, and no-JavaScript
      checks for core reading paths
- [ ] Test interactive Svelte components through Vitest browser mode with the
      Playwright provider and `vitest-browser-svelte`
- [ ] Add Playwright screenshot coverage for the landing page and representative
      documentation pages at fixed mobile and desktop viewports in both themes

Verification:

- `pnpm --filter @arclightning/website check`
- `pnpm --filter @arclightning/website test`
- `pnpm --filter @arclightning/website test:browser`
- `pnpm --filter @arclightning/website test:e2e`
- `pnpm --filter @arclightning/website test:visual`
- `pnpm --filter @arclightning/website build`
- Review Playwright screenshot diffs for mobile and desktop widths in both
  themes

## T15: Verify the integrated release

Close gaps in the specified workflows and prepare one coherent release across
the CLI, desktop app, MCP server, website, and repository-native mode.

Blocked by: T07, T09, T10, T13, T14

Acceptance criteria:

- [ ] Run one project through capture, specification, planning, ready work,
      execution, handoff, completion, repository synchronization, and conflict
      recovery across the supported interfaces
- [ ] Verify equivalent operations return the same semantic result through CLI,
      desktop, and MCP adapters
- [ ] Document installation, project migration, desktop use, CLI automation,
      MCP, repository-native collaboration, and recovery
- [ ] Map every success criterion in `SPEC.md` to automated evidence or a named
      human review
- [ ] Review and approve intentional Playwright screenshot changes before
      updating baselines
- [ ] Pass all formatting, lint, type, test, build, and documentation checks from
      a clean checkout

Verification:

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `pnpm -r check`
- `pnpm -r test`
- `pnpm -r build`
