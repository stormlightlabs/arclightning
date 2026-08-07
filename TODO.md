# Arc Lightning v1 Implementation Tickets

Source specification: [ROADMAP.md](ROADMAP.md)

## Milestone 1: Local tracker

Exit criterion: a developer can initialize Arc Lightning, capture ideas, organize
spec-backed work through subtasks, manage lifecycle and dependencies, and query ready
work without snapshots.

### T01: Establish the synchronous Rust application foundation

Prepare the crate for incremental feature work with the approved dependencies,
module boundaries, error strategy, and a CLI-focused test harness.

### T02: Initialize a Git-aware Arc Lightning project

Let a developer initialize and rediscover an Arc Lightning project from anywhere
in a non-bare Git worktree.

### T03: Capture and manage ideas

Give developers a local inbox for creating, editing, listing, and discarding ideas
with Markdown descriptions.

### T04: Create releases and spec-backed epics

Let developers group work into releases and register one epic for each existing Markdown spec.

### T05: Organize milestones, tasks, and subtasks

Let developers decompose an epic into ordered milestones, tasks, and independently
tracked subtasks.

### T06: Enforce lifecycle and parking behavior

Lets developers start, park, unpark, complete, and cancel work while preserving the roadmap's lifecycle invariants.

### T07: Model blockers and compute ready work

Lets developers express task dependencies and obtain deterministic recommendations for actionable leaf work.

### T08: Inspect and validate the local work graph

Gives humans and agents consistent read commands for records, filtered collections,
hierarchy, progress, and integrity problems.

### T09: Promote ideas into linked epics

Turns a captured idea into a spec-backed epic while retaining an explicit provenance link.

### T10: Preserve task handoffs and completion evidence

**What to build:** Let an agent park active work with one current resume note and attach optional Markdown evidence when completing a task.

**Blocked by:** T06, T08

**Acceptance criteria:**

- [x] Add non-null Markdown `handoff` and `evidence` fields to tasks with empty defaults.
- [x] Implement `arcl task handoff <id> --note <markdown>|--note-file <path|->` for in-progress tasks.
- [x] Store the handoff and park the task in one transaction; failures leave both unchanged.
- [x] Retain the current handoff through unpark and start transitions until another handoff replaces it.
- [x] Extend task completion with mutually exclusive `--evidence` and `--evidence-file <path|->` inputs.
- [x] Store completion evidence in the same transaction as the terminal transition without treating it as proof or executing its contents.
- [x] Include handoff and relevant blocker evidence in show, context, plain, and JSON output.

**Verification:**

- `cargo test --workspace --all-features handoff`
- `cargo test --workspace --all-features evidence`
- Verify transaction rollback for invalid states, unreadable files, malformed UTF-8, and guarded parent completion.
- Park, resume, and complete a task through the binary and compare human and JSON context output.

## Milestone 2: Version-controlled snapshots

Exit criterion: a snapshot-enabled project can round-trip its complete graph through
human-editable files, follow ordinary branch changes automatically, and stop safely
when local and snapshot state diverge.

### T11: Encode canonical TOML-front-matter snapshots

**What to build:** Define a deterministic parser and renderer for every snapshot record
and the versioned snapshot manifest.

**Blocked by:** T07, T09, T10

**Acceptance criteria:**

- [x] Parse and render the exact `+++`-delimited TOML-front-matter and Markdown-body format from the roadmap.
- [x] Support idea, release, epic, milestone, task, and subtask records with kebab-case fields.
- [x] Preserve optional task handoff and evidence fields through canonical snapshot round trips.
- [x] Use `<id>.md` filenames in the correct entity directory and keep subtasks in `tasks/`.
- [x] Render optional fields by omission, sort ID arrays, use LF endings, and write one trailing newline.
- [x] Reject malformed delimiters, invalid TOML, unknown v1 fields, invalid UTF-8,
      mismatched directory/filename/ID type, and unsupported manifest versions.
- [x] Preserve Markdown bodies exactly except for the documented final-newline normalization.
- [x] Canonical rendering is deterministic and parse-render-parse stable.

**Verification:**

- `cargo test --workspace --all-features snapshot::codec`
- Round-trip representative Unicode and multiline records for every entity type.
- Run golden-file tests for canonical output and each malformed input class.

### T12: Export database state as isolated snapshot records

**What to build:** Let a snapshot-enabled project write its complete database projection
into deterministic, independently mergeable record files.

**Blocked by:** T11

**Acceptance criteria:**

- [x] `arcl init --snapshot` enables the default snapshot path and creates `manifest.toml` with format version 1.
- [x] Add the `snapshot_files` migration and store the exact files from the last successful synchronization as the merge base.
- [x] Implement `arcl snapshot export` for an initial export and later changed-record exports.
- [x] Write through same-directory temporary files and atomic per-file renames;
      update the base only after all required exports succeed.
- [x] Verify each destination still matches the content observed at command start before replacing it.
- [x] Exporting unchanged state produces no content changes.
- [x] The exporter never writes the SQLite database or volatile generation metadata into the snapshot.

**Verification:**

- `cargo test --workspace --all-features snapshot::export`
- Export a populated graph twice and verify the second run changes no bytes.
- Simulate a concurrent file edit before replacement and verify a conflict rather than overwrite.

### T13: Import and rebuild from a validated snapshot

**What to build:** Let a developer edit snapshot files or clone a repository and reconstruct the complete SQLite work graph safely.

**Blocked by:** T12

**Acceptance criteria:**

- [x] Parse every candidate record before opening a write transaction.
- [x] Validate IDs, required fields, enums, spec paths, relationships, promotion symmetry,
      parentage, cycles, lifecycle, and existing-record removal as one complete graph.
- [x] Implement `arcl snapshot import` as an all-or-nothing database replacement or update followed by base-state refresh.
- [x] Rebuild an absent or empty local database from a valid snapshot.
- [x] Accept manually added records with valid prefixed ULIDs.
- [x] Reject missing or renamed existing IDs because v1 has no hard deletion.
- [x] Normalize harmless formatting only after a successful semantic import.
- [x] Report the source file, field, and relationship path for validation failures.

**Verification:**

- `cargo test --workspace --all-features snapshot::import`
- Rebuild a fresh database from an exported snapshot and compare every semantic record and relationship.
- Corrupt each validation class and verify neither SQLite nor snapshot files change.

### T14: Reconcile database and snapshot changes automatically

**What to build:** Apply the roadmap's three-way synchronization state machine before
ordinary commands and recover safe one-sided changes automatically.

**Blocked by:** T13

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

### T15: Diagnose and resolve snapshot and Git conflicts

**What to build:** Give developers enough status and recovery tooling to understand branch-driven
changes and explicitly resolve genuine divergence.

**Blocked by:** T14

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

### T16: Check, diff, and apply additive plans transactionally

**What to build:** Validate, preview, and apply a versioned TOML plan containing milestones, tasks, subtasks, and dependencies under an existing epic.

**Blocked by:** T07

**Acceptance criteria:**

- [ ] Parse format-version-1 plans from a path or stdin with recursive subtasks and plan-local dependency references.
- [ ] Enforce milestone plan-key uniqueness within an epic and task plan-key uniqueness within a milestone.
- [ ] Resolve `<milestone>/<task>/...` references and full existing task IDs before writing.
- [ ] Validate the complete resulting hierarchy and dependency graph transactionally.
- [ ] Implement `arcl plan check` with complete validation and no writes.
- [ ] Implement `arcl plan diff` with deterministic create and update output and no writes.
- [ ] Create ULIDs for new plan keys and update matching records on repeated application.
- [ ] Leave omitted records unchanged and never complete, cancel, move, or delete them implicitly.
- [ ] Implement `arcl plan apply` as one transaction after the same validation used by check and diff.
- [ ] Applying the same plan twice produces no duplicate records or second-run changes.

**Verification:**

- `cargo test --workspace --all-features plan`
- Check and diff the roadmap example from a file and stdin, apply it twice, and compare database state.
- Verify invalid references, duplicate keys, cycles, and partial-update failures roll back fully.

## Milestone 4: CLI and release hardening

Exit criterion: the complete v1 workflow satisfies the roadmap's CLI contract,
passes all automated checks, and can be followed using terminal help and project documentation.

### T17: Make the CLI consistent for humans and automation

**What to build:** Apply the shared command, output, color, help, diagnostic, and exit-code
contracts across every implemented workflow.

**Blocked by:** T08, T09, T10, T15, T16

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

### T18: Verify and document the complete v1 workflow

**What to build:** Close the gaps found by a full-system review and leave a release-ready CLI with accurate user documentation.

**Blocked by:** T17

**Acceptance criteria:**

- [ ] Add an end-to-end test covering init, idea capture, promotion, plan application, ready work,
      readiness explanation, context retrieval, handoff, completion evidence, lifecycle transitions,
      snapshot export, manual snapshot edit, branch-driven import, and conflict recovery.
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
