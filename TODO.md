# Arc Lightning v1 Implementation Tickets

Source specification: [ROADMAP.md](ROADMAP.md)

## Milestone 1: Local tracker

Exit criterion: a developer can initialize Arc Lightning, capture ideas, organize
spec-backed work through subtasks, manage lifecycle and dependencies, and query ready
work without snapshots.

### T01: Establish the synchronous Rust application foundation

**What to build:** Prepare the crate for incremental feature work with the approved
dependencies, module boundaries, error strategy, and a CLI-focused test harness.

**Blocked by:** None - can start immediately

**Acceptance criteria:**

- [x] Add the production dependencies and feature selections approved in the roadmap,
      including `clap`, `gix`, `owo-colors`, `rusqlite`, `serde`, `toml`, `ulid`, `thiserror`, and `anyhow`.
- [x] Keep the application synchronous and omit Tokio and all `gix` network-client features.
- [x] Establish the roadmap's application, domain, storage, snapshot, plan, output, and
      VCS module boundaries without placeholder production APIs.
- [x] Restrict `anyhow` to the binary/application boundary and define typed infrastructure
      and domain error categories with `thiserror`.
- [x] Add the minimum test-only dependencies and helpers needed to run the binary in isolated temporary directories.
- [x] `arcl --help` and `arcl --version` succeed from the compiled binary.
- [x] Review `cargo tree -e features` and document the selected minimal `gix` feature set.

**Verification:**

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo tree -e features`

### T02: Initialize a Git-aware Arc Lightning project

**What to build:** Let a developer initialize and rediscover an Arc Lightning project from
anywhere in a non-bare Git worktree.

**Blocked by:** T01

**Acceptance criteria:**

- [ ] Discover the enclosing worktree with `gix` from the root or any descendant directory.
- [ ] `arcl init` creates `.arcl/config.toml`, `.arcl/.gitignore`, and a migrated local
      `.arcl/arcl.db` without changing the root `.gitignore`.
- [ ] `.arcl/.gitignore` covers the database, SQLite side files, temporary files, and
      conflict artifacts specified by the roadmap.
- [ ] Initialization enables SQLite foreign keys, configures a finite busy timeout, and
      applies embedded numbered migrations using `PRAGMA user_version`.
- [ ] Repeating initialization is safe and does not replace valid configuration or data.
- [ ] Operation outside Git and inside a bare repository fails with a helpful error and
      the specified exit-code category.
- [ ] Unborn and detached `HEAD` states do not panic.
- [ ] Arc Lightning does not invoke the `git` executable or mutate refs, the index, remotes, or Git configuration.

**Verification:**

- `cargo test --workspace --all-features init`
- Manually run `arcl init` from a repository root and nested directory, then inspect `.arcl/`.
- Compare Git refs, index state, remotes, and configuration before and after initialization.

### T03: Capture and manage ideas

**What to build:** Give developers a local inbox for creating, editing, listing, and
discarding ideas with Markdown descriptions.

**Blocked by:** T02

**Acceptance criteria:**

- [ ] Add validated `arcl-i-<ulid>` IDs and the `ideas` migration from the roadmap.
- [ ] Implement `arcl idea create`, `arcl idea update`, and `arcl idea discard` with the specified lifecycle rules.
- [ ] Support inline descriptions and UTF-8 descriptions read from a file or standard input.
- [ ] Reject empty titles, malformed IDs, invalid transitions, and ambiguous description inputs without partial writes.
- [ ] Repeating discard is idempotent; discarded ideas cannot return to `captured`.
- [ ] Mutations print useful human output and expose the created or updated record as versioned JSON.

**Verification:**

- `cargo test --workspace --all-features idea`
- Create, update, and discard an idea through the compiled binary and inspect the SQLite row.
- Pipe a multiline Markdown description through stdin and verify an exact round trip.

### T04: Create releases and spec-backed epics

**What to build:** Let developers group work into releases and register one epic for each existing Markdown spec.

**Blocked by:** T02

**Acceptance criteria:**

- [ ] Add validated release and epic IDs and the corresponding roadmap migrations.
- [ ] Implement release create and update commands.
- [ ] Implement epic create and update commands, including optional release association.
- [ ] Require each epic to reference one unique, existing regular `.md` file inside the worktree.
- [ ] Normalize stored spec paths relative to the worktree and reject absolute paths,
      traversal, symlink escapes, duplicates, and non-Markdown targets.
- [ ] Updating an epic never edits the linked spec.
- [ ] Missing or invalid release associations roll back the entire mutation.

**Verification:**

- `cargo test --workspace --all-features epic`
- `cargo test --workspace --all-features release`
- Create an epic from a nested working directory, then verify its stored path is root-relative.
- Exercise traversal, symlink escape, duplicate-spec, and missing-file failures.

### T05: Organize milestones, tasks, and subtasks

**What to build:** Let developers decompose an epic into ordered milestones, tasks, and independently tracked subtasks.

**Blocked by:** T04

**Acceptance criteria:**

- [ ] Add validated milestone and task IDs and the corresponding roadmap migrations.
- [ ] Implement milestone create and update commands with epic ownership and non-negative position.
- [ ] Implement task create and update commands with milestone, optional parent, priority, position, title, and Markdown description.
- [ ] Represent subtasks as task rows with `parent_id`; do not infer tracked work from Markdown checkboxes.
- [ ] Enforce same-milestone parentage, reject self-parenting and parent cycles, and validate a moved subtree before changing milestone or parent.
- [ ] Accept duplicate positions and sort ties by ULID.
- [ ] Roll back multi-row moves when any descendant would violate an invariant.

**Verification:**

- `cargo test --workspace --all-features milestone`
- `cargo test --workspace --all-features task`
- Build an epic, milestone, task, and two-level subtask hierarchy through the binary.
- Attempt cross-milestone and cyclic reparenting and confirm no rows change.

### T06: Enforce lifecycle and parking behavior

**What to build:** Let developers start, park, unpark, complete, and cancel work while preserving the roadmap's lifecycle invariants.

**Blocked by:** T05

**Acceptance criteria:**

- [ ] Implement task start, park, unpark, complete, and cancel commands with the exact allowed transitions.
- [ ] Unparking always returns a task to `pending`; completing a parked task fails until it is unparked.
- [ ] Repeating the same terminal transition is idempotent; changing between terminal states fails.
- [ ] Implement release, epic, and milestone complete and cancel commands.
- [ ] Completing or cancelling any container, including a parent task, fails while descendants are non-terminal unless `--allow-open-children` is present.
- [ ] The override changes only the selected record and never cascades state silently.
- [ ] Lifecycle failures leave the database unchanged.

**Verification:**

- `cargo test --workspace --all-features lifecycle`
- Exercise every allowed and rejected task transition through the binary.
- Verify container guards and overrides against a hierarchy with open descendants.

### T07: Model blockers and compute ready work

**What to build:** Let developers express task dependencies and obtain deterministic recommendations for actionable leaf work.

**Blocked by:** T05, T06

**Acceptance criteria:**

- [ ] Add the `task_dependencies` migration with foreign keys and composite uniqueness.
- [ ] Implement dependency add and remove commands.
- [ ] Reject missing targets, self-dependencies, duplicates, and direct or transitive dependency cycles before writing.
- [ ] Compute blocked and ready state instead of storing either as a task status.
- [ ] A cancelled blocker remains unsatisfied; only a completed blocker satisfies a dependency.
- [ ] Exclude parent tasks with non-terminal children and descendants of parked or terminal ancestors from ready work.
- [ ] Respect open container state and sort ready work by priority, milestone position, task position, and ULID.
- [ ] Implement `arcl ready` and `arcl next`, including the empty-result case.

**Verification:**

- `cargo test --workspace --all-features dependency`
- `cargo test --workspace --all-features ready`
- Construct a graph that covers completed, cancelled, parked, cyclic, parent, and cross-milestone cases and verify output order.

### T08: Inspect and validate the local work graph

**What to build:** Give humans and agents consistent read commands for records, filtered collections, hierarchy, progress, and integrity problems.

**Blocked by:** T03, T07

**Acceptance criteria:**

- [ ] Implement prefix-routed `arcl show <id>` for every entity type.
- [ ] Implement `arcl list` with all applicable kind, status, priority, release, epic, milestone, and parent filters.
- [ ] Implement `arcl tree [<id>]` with deterministic hierarchy ordering.
- [ ] Include derived blocked state, blocker details, descendant progress, and relevant associations without issuing per-record query loops.
- [ ] Implement the database and graph portion of `arcl check`, including duplicate-position warnings and all hierarchy and dependency invariants.
- [ ] Human, plain, and JSON output contain equivalent records even if their presentation differs.
- [ ] Not-found and invalid-filter failures use the specified exit-code categories.

**Verification:**

- `cargo test --workspace --all-features query`
- `cargo test --workspace --all-features tree`
- `cargo test --workspace --all-features check`
- Compare human, plain, and JSON results for the same populated graph.

### T09: Promote ideas into linked epics

**What to build:** Turn a captured idea into a spec-backed epic while retaining an explicit provenance link.

**Blocked by:** T03, T04

**Acceptance criteria:**

- [ ] Add the `idea_promotions` migration with one-to-one constraints.
- [ ] Implement `arcl idea promote <id> --spec <path> [--release <id>]`.
- [ ] Create the epic, relationship, and promoted idea state in one transaction.
- [ ] Apply the same spec-path and release validation used by direct epic creation.
- [ ] Repeating promotion returns the existing epic without generating a new ULID or changing its relationships.
- [ ] A discarded or already inconsistently linked idea cannot be promoted.
- [ ] Show and list output expose `promoted-to` and `source-idea` in both directions.

**Verification:**

- `cargo test --workspace --all-features promotion`
- Promote one idea twice and verify exactly one epic and one relationship exist.
- Force each validation failure and verify the idea remains captured.

## Milestone 2: Version-controlled snapshots

Exit criterion: a snapshot-enabled project can round-trip its complete graph through
human-editable files, follow ordinary branch changes automatically, and stop safely
when local and snapshot state diverge.

### T10: Encode canonical TOML-front-matter snapshots

**What to build:** Define a deterministic parser and renderer for every snapshot record
and the versioned snapshot manifest.

**Blocked by:** T07, T09

**Acceptance criteria:**

- [ ] Parse and render the exact `+++`-delimited TOML-front-matter and Markdown-body format from the roadmap.
- [ ] Support idea, release, epic, milestone, task, and subtask records with kebab-case fields.
- [ ] Use `<id>.md` filenames in the correct entity directory and keep subtasks in `tasks/`.
- [ ] Render optional fields by omission, sort ID arrays, use LF endings, and write one trailing newline.
- [ ] Reject malformed delimiters, invalid TOML, unknown v1 fields, invalid UTF-8,
      mismatched directory/filename/ID type, and unsupported manifest versions.
- [ ] Preserve Markdown bodies exactly except for the documented final-newline normalization.
- [ ] Canonical rendering is deterministic and parse-render-parse stable.

**Verification:**

- `cargo test --workspace --all-features snapshot::codec`
- Round-trip representative Unicode and multiline records for every entity type.
- Run golden-file tests for canonical output and each malformed input class.

### T11: Export database state as isolated snapshot records

**What to build:** Let a snapshot-enabled project write its complete database projection
into deterministic, independently mergeable record files.

**Blocked by:** T10

**Acceptance criteria:**

- [ ] `arcl init --snapshot` enables the default snapshot path and creates `manifest.toml` with format version 1.
- [ ] Add the `snapshot_base` migration and store the exact files from the last successful synchronization.
- [ ] Implement `arcl snapshot export` for an initial export and later changed-record exports.
- [ ] Write through same-directory temporary files and atomic per-file renames;
      update the base only after all required exports succeed.
- [ ] Verify each destination still matches the content observed at command start before replacing it.
- [ ] Exporting unchanged state produces no content changes.
- [ ] The exporter never writes the SQLite database or volatile generation metadata into the snapshot.

**Verification:**

- `cargo test --workspace --all-features snapshot::export`
- Export a populated graph twice and verify the second run changes no bytes.
- Simulate a concurrent file edit before replacement and verify a conflict rather than overwrite.

### T12: Import and rebuild from a validated snapshot

**What to build:** Let a developer edit snapshot files or clone a repository and reconstruct the complete SQLite work graph safely.

**Blocked by:** T11

**Acceptance criteria:**

- [ ] Parse every candidate record before opening a write transaction.
- [ ] Validate IDs, required fields, enums, spec paths, relationships, promotion symmetry,
      parentage, cycles, lifecycle, and existing-record removal as one complete graph.
- [ ] Implement `arcl snapshot import` as an all-or-nothing database replacement or update followed by base-state refresh.
- [ ] Rebuild an absent or empty local database from a valid snapshot.
- [ ] Accept manually added records with valid prefixed ULIDs.
- [ ] Reject missing or renamed existing IDs because v1 has no hard deletion.
- [ ] Normalize harmless formatting only after a successful semantic import.
- [ ] Report the source file, field, and relationship path for validation failures.

**Verification:**

- `cargo test --workspace --all-features snapshot::import`
- Rebuild a fresh database from an exported snapshot and compare every semantic record and relationship.
- Corrupt each validation class and verify neither SQLite nor snapshot files change.

### T13: Reconcile database and snapshot changes automatically

**What to build:** Apply the roadmap's three-way synchronization state machine before
ordinary commands and recover safe one-sided changes automatically.

**Blocked by:** T12

**Acceptance criteria:**

- [ ] Compare the stored base, current database projection, and current snapshot projection semantically as specified.
- [ ] Implement all five state-machine outcomes, including the equal-both-sides case.
- [ ] Import snapshot-only changes before normal reads or writes.
- [ ] Export database-only changes, including recovery after a database commit followed by an interrupted export.
- [ ] Refuse automatic writes when database and snapshot both diverged differently from the base.
- [ ] Bypass automatic reconciliation for explicit snapshot commands and make `arcl check`
      report divergence without selecting a side.
- [ ] Recheck observed snapshot content before post-mutation export so a concurrent editor cannot be overwritten.

**Verification:**

- `cargo test --workspace --all-features snapshot::sync`
- Use failure injection to stop after the database commit, during file export, and before base update,
  then verify the next command's behavior.
- Cover every state-machine row with an end-to-end test.

### T14: Diagnose and resolve snapshot and Git conflicts

**What to build:** Give developers enough status and recovery tooling to understand branch-driven
changes and explicitly resolve genuine divergence.

**Blocked by:** T13

**Acceptance criteria:**

- [ ] Implement `arcl snapshot status` with record-level database, snapshot, and base differences.
- [ ] Complete `arcl status` and `arcl check` with `gix` path states for configuration and snapshot files.
- [ ] Detect unmerged snapshot paths and common conflict markers before TOML parsing.
- [ ] Implement explicit export, import, and `snapshot reconcile --use database|snapshot` recovery paths.
- [ ] Show a destructive-side summary and require confirmation in a TTY; require `--force` when non-interactive.
- [ ] Forced recovery updates the chosen side and common base without invoking a Git mutation.
- [ ] Branch checkout, detached HEAD, and unborn HEAD changes are handled without assuming a branch name exists.

**Verification:**

- `cargo test --workspace --all-features snapshot::reconcile`
- `cargo test --workspace --all-features vcs`
- Exercise external edits, a branch switch, an unresolved merge, and dual-side divergence in disposable repositories.
- Verify refs, index, remotes, and Git configuration are unchanged after every recovery command.

## Milestone 3: Repeatable planning

Exit criterion: an external tool or coding agent can apply a complete TOML plan to an
epic repeatedly without duplicating or deleting work.

### T15: Apply additive plans transactionally

**What to build:** Convert a versioned TOML plan into milestones, tasks, subtasks, and dependencies under an existing epic.

**Blocked by:** T07

**Acceptance criteria:**

- [ ] Parse format-version-1 plans from a path or stdin with recursive subtasks and plan-local dependency references.
- [ ] Enforce milestone plan-key uniqueness within an epic and task plan-key uniqueness within a milestone.
- [ ] Resolve `<milestone>/<task>/...` references and full existing task IDs before writing.
- [ ] Validate the complete resulting hierarchy and dependency graph transactionally.
- [ ] Create ULIDs for new plan keys and update matching records on repeated application.
- [ ] Leave omitted records unchanged and never complete, cancel, move, or delete them implicitly.
- [ ] Implement `--dry-run` with deterministic create/update output and no writes.
- [ ] Applying the same plan twice produces no duplicate records or second-run changes.

**Verification:**

- `cargo test --workspace --all-features plan`
- Apply the roadmap example from a file and stdin, then apply it again and compare database state.
- Verify invalid references, duplicate keys, cycles, and partial-update failures roll back fully.

## Milestone 4: CLI and release hardening

Exit criterion: the complete v1 workflow satisfies the roadmap's CLI contract,
passes all automated checks, and can be followed using terminal help and project documentation.

### T16: Make the CLI consistent for humans and automation

**What to build:** Apply the shared command, output, color, help, diagnostic, and exit-code
contracts across every implemented workflow.

**Blocked by:** T08, T09, T14, T15

**Acceptance criteria:**

- [ ] Audit the noun-and-verb command hierarchy, argument names, filter combinations,
      description inputs, and help examples against the roadmap and <https://clig.dev/>.
- [ ] Make human output concise and keep primary results on stdout and diagnostics on stderr.
- [ ] Complete stable versioned JSON envelopes and one-record-per-line plain output for every read command.
- [ ] Implement quiet mutation output and reject conflicting output modes.
- [ ] Use `owo-colors` terminal-aware formatting and honor `NO_COLOR`, `FORCE_COLOR`,
      and explicit `--color auto|always|never` precedence.
- [ ] Ensure JSON, plain, quiet, piped, and ordinary non-TTY output contain no accidental ANSI escapes.
- [ ] Map usage, validation, conflict, not-found, and unexpected failures to the specified exit codes and structured JSON errors.
- [ ] Handle broken pipes quietly and never show a backtrace for an expected user error.
- [ ] Top-level and subcommand help lead with useful examples and suggest unambiguous corrections.

**Verification:**

- `cargo test --workspace --all-features cli`
- Run output-mode tests under TTY and non-TTY conditions with color environment variables.
- Exercise every exit-code category and inspect stdout/stderr separation.

### T17: Verify and document the complete v1 workflow

**What to build:** Close the gaps found by a full-system review and leave a release-ready CLI with accurate user documentation.

**Blocked by:** T16

**Acceptance criteria:**

- [ ] Add an end-to-end test covering init, idea capture, promotion, plan application, ready work,
      lifecycle transitions, snapshot export, manual snapshot edit, branch-driven import, and conflict recovery.
- [ ] Add failure-injection coverage for every snapshot/database atomicity boundary identified in the roadmap.
- [ ] Verify tests do not depend on global Git configuration, the developer's home directory, locale-specific output, or wall-clock sleeps.
- [ ] Verify Arc Lightning never invokes the `git` executable and never changes Git refs, index, remotes, or configuration.
- [ ] Update the README with installation, quick start, snapshot collaboration, recovery, JSON automation, and links to the roadmap.
- [ ] Ensure terminal help is sufficient to complete the common workflow without reading source code.
- [ ] Review implementation behavior against every roadmap success criterion and record any approved spec correction in `ROADMAP.md`.
- [ ] All required formatting, check, lint, test, and documentation commands pass from a clean worktree.

**Verification:**

- `cargo fmt --all -- --check`
- `cargo check --workspace --all-targets --all-features`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- `cargo doc --workspace --all-features --no-deps`
- Run the roadmap's human review workflows in a disposable repository.

## Frontier

T01 is the only ticket that can start immediately. After T01, T02 becomes available. After T02, T03 and T04 can proceed independently.

Use one fresh implementation context per ticket. Re-read [ROADMAP.md](ROADMAP.md) and the relevant completed ticket changes before starting each one.
