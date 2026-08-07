# Arc Lightning v1 Specification

Status: Ready

## Objective

Arc Lightning (`arcl`) is a Git-aware command-line task tracker for developers and coding agents.

It keeps the live work graph in a local SQLite database and can project that graph into an optional,
human-editable snapshot suitable for version control.

The first release must support a complete personal planning loop:

1. Capture an idea.
2. Promote the idea into an epic linked to a Markdown specification.
3. Group the epic's work into milestones.
4. Decompose milestones into tasks and subtasks.
5. Express blocking dependencies.
6. Find ready work deterministically.
7. Give an agent the task, planning context, blockers, and latest handoff in one bounded response.
8. Preserve a handoff when work pauses and evidence when work completes.
9. Track work through completion or cancellation.
10. Share and merge the work graph through Git when snapshots are enabled.

Arc Lightning provides task-tracking mechanics. It does not prescribe how a developer interviews stakeholders,
writes specifications, or produces implementation plans.

## Users and use cases

The primary user is a developer who plans work in Markdown, works from a terminal, and may delegate implementation to coding agents.

The developer must be able to:

- use Arc Lightning from any directory inside a Git worktree;
- keep private task state locally when snapshots are disabled;
- opt into a readable, mergeable representation of task state in Git;
- inspect and edit snapshot records without installing Arc Lightning;
- recover a local database after cloning or changing branches;
- capture ideas without assigning them to planned work;
- retain provenance when promoting an idea into a spec-backed epic;
- organize an epic into ordered milestones, tasks, and subtasks;
- model blockers without manually maintaining a ready queue;
- explain why a task is ready or blocked;
- retrieve a bounded task context packet without assembling the hierarchy through several commands;
- leave a current handoff for the next developer or agent and attach evidence to completed work;
- obtain concise human output or stable JSON for scripts and agents; and
- detect malformed records, broken links, cycles, and synchronization conflicts before data is overwritten.

## Success criteria

v1 is usable when all of the following are true:

- `arcl init` initializes a project inside a non-bare Git worktree without running the `git` executable
  or modifying Git history, the index, branches, or remotes.
- All live tracker entities are stored as rows in a local SQLite database.
- The SQLite database and its transient files are ignored through `.arcl/.gitignore`.
- A user can create, inspect, update, transition, and list every v1 entity through the CLI.
- One epic links to exactly one existing Markdown spec within the worktree.
- A release can contain multiple epics; an epic can contain multiple milestones; a milestone can contain tasks;
  and tasks can contain subtasks.
- Ideas can be promoted idempotently into linked epics without losing the source idea.
- Blocking and readiness are computed from validated task relationships.
- A user can explain a task's readiness and retrieve its task, ancestor, specification, dependency,
  handoff, and completion context in one read.
- Handing off active work stores a resume note and parks the task atomically.
- Completing a task can preserve Markdown evidence without making that evidence a readiness condition.
- Plan input can be checked and diffed without writing before it is applied transactionally.
- A snapshot-enabled project automatically imports external snapshot changes and exports successful
  local mutations when only one side has changed.
- Arc Lightning refuses to overwrite either side when both the database and snapshot have changed since their last common state.
- Snapshot records use TOML front matter and Markdown bodies, and a person can edit them with an ordinary text editor.
- Two Git branches that modify different records normally touch different snapshot files.
- Every read command offers stable JSON output.
- Human output respects terminal capabilities, `NO_COLOR`, `FORCE_COLOR`, and an explicit color flag.
- Unit and end-to-end CLI tests cover the domain graph, SQLite constraints, snapshot round trips,
  Git discovery, branch-change imports, and conflict handling.

## Terminology and hierarchy

The v1 hierarchy is:

```text
Release
└── Epic / Spec
    └── Milestone
        └── Task
            └── Subtask
```

### Idea

An idea is an uncommitted thought in the project inbox. It does not belong to a release, epic, or milestone.
Promotion creates a new epic and preserves a link back to the idea.

### Release

A release is a lightweight container grouping multiple epics. An epic belongs to at most one release in v1.

### Epic and spec

An epic is the tracker representation of one Markdown spec.
`spec_path` is required, unique, relative to the Git worktree root, and must resolve to an existing regular Markdown file inside the worktree.

Arc Lightning links to the spec but does not own or rewrite its contents.

### Milestone

A milestone is an ordered, lightweight container of tasks belonging to one epic.
It marks a meaningful implementation stage within that spec.

### Task and subtask

Tasks and subtasks share one table and one ID type. A row with `parent_id = NULL` is a task.
A row with a parent is a subtask. Parent and child must belong to the same milestone.

A Markdown checklist inside a description is prose. It does not create independently tracked subtasks.

## Identifiers

Every entity ID combines a type prefix with a ULID:

| Entity          | Format          |
| --------------- | --------------- |
| Idea            | `arcl-i-<ulid>` |
| Release         | `arcl-r-<ulid>` |
| Epic            | `arcl-e-<ulid>` |
| Milestone       | `arcl-m-<ulid>` |
| Task or subtask | `arcl-t-<ulid>` |

Use `ulid::Ulid` from `dylanhart/ulid-rs` with its `serde` feature.
Serialize the suffix using the crate's canonical 26-character string representation.
Store the full prefixed ID as `TEXT` in SQLite and expose separate validated Rust newtypes such as `IdeaId` and `TaskId`.

IDs are immutable. Importing a snapshot that removes or changes an existing ID is a validation error because v1 has no hard-delete operation.

## Lifecycle

### Idea status

```text
captured ──> promoted
    └──────> discarded
```

- `captured` is the default.
- `promoted` requires exactly one linked epic.
- `discarded` is terminal.
- Repeating promotion returns the existing epic and makes no new rows.

### Task status

```text
pending ──> in_progress ──> completed
   │              │
   ├──> parked <──┤
   └──────────────┴──────> cancelled
```

- `pending` is the default.
- `start` changes `pending` to `in_progress` and is idempotent when already in progress.
- `park` accepts `pending` or `in_progress`; `unpark` always returns the task to `pending` so v1 does not need hidden previous-state metadata.
- `parked` excludes the task and its descendants from ready work.
- `complete` accepts `pending` or `in_progress`; a parked task must be unparked first.
- `cancel` accepts any non-terminal task status.
- `completed` and `cancelled` are terminal.
- Repeating the same terminal transition is idempotent; changing from one terminal state to the other fails.
- Terminal work cannot be reopened.

### Container status

Releases, epics, and milestones use `open`, `completed`, and `cancelled`.

- `open` is the default.
- Completing or cancelling a container with non-terminal descendants fails unless the user supplies the documented override flag.
- The override transitions the container only; it never silently changes descendant states.

### Derived state

`ready` and `blocked` are query results, not stored statuses.

A task is ready when:

- its status is `pending`;
- it has no non-terminal children;
- none of its ancestors is parked, completed, or cancelled;
- every direct blocker is `completed`; and
- its milestone, epic, and release, when present, are open.

A task is blocked when at least one direct blocker is not completed.
A cancelled blocker remains unsatisfied so cancellation cannot silently authorize dependent work.

Parent tasks with children are planning containers and do not appear in ready results.

Completing or cancelling one with non-terminal children requires the same explicit override used by other containers.

## Priority and ordering

Task priority is one of `critical`, `high`, `normal`, or `low`; `normal` is the default.

Milestones and tasks have an integer `position` within their parent.
Position controls display order, while ULID provides a deterministic tie-breaker.
Duplicate positions are valid after merges and produce a warning from `arcl check`, not data loss or an automatic rewrite.

Ready work sorts by:

1. priority (`critical` first);
2. milestone position;
3. task position; and
4. ULID.

## SQLite storage

The local database path defaults to `.arcl/arcl.db`. SQLite is the live operational store and is never intended for Git.

The application must enable foreign keys, set a finite busy timeout, use explicit transactions
for mutations, and run embedded numbered migrations.

Schema versioning uses `PRAGMA user_version`.

v1 requires these tables.

### `meta`

| Column  | Type   | Rules       |
| ------- | ------ | ----------- |
| `key`   | `TEXT` | primary key |
| `value` | `TEXT` | not null    |

Reserved keys include the database format version and snapshot synchronization state.

### `ideas`

| Column        | Type   | Rules                                  |
| ------------- | ------ | -------------------------------------- |
| `id`          | `TEXT` | primary key, validated `arcl-i` ID     |
| `title`       | `TEXT` | not null, non-empty                    |
| `description` | `TEXT` | not null, Markdown, default empty      |
| `status`      | `TEXT` | `captured`, `promoted`, or `discarded` |

### `releases`

| Column        | Type   | Rules                               |
| ------------- | ------ | ----------------------------------- |
| `id`          | `TEXT` | primary key, validated `arcl-r` ID  |
| `title`       | `TEXT` | not null, non-empty                 |
| `description` | `TEXT` | not null, Markdown, default empty   |
| `status`      | `TEXT` | `open`, `completed`, or `cancelled` |

### `epics`

| Column        | Type   | Rules                                                 |
| ------------- | ------ | ----------------------------------------------------- |
| `id`          | `TEXT` | primary key, validated `arcl-e` ID                    |
| `release_id`  | `TEXT` | nullable foreign key to `releases`, restrict deletion |
| `title`       | `TEXT` | not null, non-empty                                   |
| `description` | `TEXT` | not null, Markdown, default empty                     |
| `spec_path`   | `TEXT` | not null, unique, normalized worktree-relative path   |
| `status`      | `TEXT` | `open`, `completed`, or `cancelled`                   |

### `idea_promotions`

| Column    | Type   | Rules                               |
| --------- | ------ | ----------------------------------- |
| `idea_id` | `TEXT` | primary key, foreign key to `ideas` |
| `epic_id` | `TEXT` | unique, foreign key to `epics`      |

Promotion creates the epic, inserts this relationship, and changes the idea status in one transaction.

### `milestones`

| Column        | Type      | Rules                               |
| ------------- | --------- | ----------------------------------- |
| `id`          | `TEXT`    | primary key, validated `arcl-m` ID  |
| `epic_id`     | `TEXT`    | not null, foreign key to `epics`    |
| `plan_key`    | `TEXT`    | nullable, unique within the epic    |
| `title`       | `TEXT`    | not null, non-empty                 |
| `description` | `TEXT`    | not null, Markdown, default empty   |
| `status`      | `TEXT`    | `open`, `completed`, or `cancelled` |
| `position`    | `INTEGER` | not null, non-negative              |

### `tasks`

| Column         | Type      | Rules                                              |
| -------------- | --------- | -------------------------------------------------- |
| `id`           | `TEXT`    | primary key, validated `arcl-t` ID                 |
| `milestone_id` | `TEXT`    | not null, foreign key to `milestones`              |
| `parent_id`    | `TEXT`    | nullable foreign key to `tasks`, restrict deletion |
| `plan_key`     | `TEXT`    | nullable, unique within the milestone              |
| `title`        | `TEXT`    | not null, non-empty                                |
| `description`  | `TEXT`    | not null, Markdown, default empty                  |
| `status`       | `TEXT`    | task lifecycle value                               |
| `priority`     | `TEXT`    | priority value                                     |
| `position`     | `INTEGER` | not null, non-negative                             |
| `handoff`      | `TEXT`    | not null, Markdown, default empty                  |
| `evidence`     | `TEXT`    | not null, Markdown, default empty                  |

The domain layer must enforce same-milestone parentage and prevent parent cycles before writing.

### `task_dependencies`

| Column       | Type   | Rules                  |
| ------------ | ------ | ---------------------- |
| `task_id`    | `TEXT` | foreign key to `tasks` |
| `blocker_id` | `TEXT` | foreign key to `tasks` |

The composite primary key is `(task_id, blocker_id)`. Self-dependencies and dependency cycles are rejected by the domain layer.

### `snapshot_base`

| Column    | Type   | Rules                                                  |
| --------- | ------ | ------------------------------------------------------ |
| `path`    | `TEXT` | primary key, relative to snapshot root                 |
| `content` | `BLOB` | exact content from the last successful synchronization |

This table provides the common base for three-way synchronization without relying on timestamps or unstable hashes.

## Snapshot format

Snapshots are opt-in. `arcl init --snapshot` enables the default path `.arcl/snapshot`;
configuration may choose another worktree-relative path.

The default layout is:

```text
.arcl/
├── .gitignore
├── config.toml
├── arcl.db
└── snapshot/
    ├── manifest.toml
    ├── ideas/
    ├── releases/
    ├── epics/
    ├── milestones/
    └── tasks/
```

Subtasks live in `tasks/`. Each filename is exactly `<id>.md`. Titles do not appear in filenames, avoiding renames when titles change.

`manifest.toml` contains only stable format metadata:

```toml
format-version = 1
```

It must not contain a generated timestamp, record inventory, or other globally changing data that would turn every branch edit into a conflict.

### Record encoding

Every record contains TOML front matter delimited by lines containing exactly `+++`, followed by the Markdown description.
Front-matter keys use kebab case. Writers use LF line endings and one trailing newline.
Readers accept a missing final newline but normalize it on the next export.

Example task:

```markdown
+++
id = "arcl-t-01K0B3N4QSC9R7K6W8X2M5YH1Z"
title = "Implement ready-work query"
status = "pending"
priority = "high"
milestone = "arcl-m-01K0B2ZWTX7JX9PH7W5G1S6A9Q"
position = 20
blocked-by = ["arcl-t-01K0B31M6VGK4YH8VKT4C0D2DR"]
+++

Return actionable tasks with no unfinished blockers.

## Acceptance criteria

- Closed blockers do not prevent readiness.
- Output order is deterministic.
```

Optional values are omitted rather than serialized as empty strings. Arrays are sorted by ID during export.
The Markdown body maps directly to the entity's SQLite `description` field.

Required record fields and relationships are:

| Directory    | Required front matter                                        | Optional front matter              |
| ------------ | ------------------------------------------------------------ | ---------------------------------- |
| `ideas`      | `id`, `title`, `status`                                      | `promoted-to`                      |
| `releases`   | `id`, `title`, `status`                                      | none                               |
| `epics`      | `id`, `title`, `status`, `spec-path`                         | `release`, `source-idea`           |
| `milestones` | `id`, `title`, `status`, `epic`, `position`                  | `plan-key`                         |
| `tasks`      | `id`, `title`, `status`, `priority`, `milestone`, `position` | `parent`, `plan-key`, `blocked-by`, `handoff`, `evidence` |

The directory, filename, ID prefix, and decoded entity kind must agree.

### Snapshot validation

Before importing any records, Arc Lightning validates the complete candidate graph:

- manifest version is supported;
- front matter parses as TOML;
- required fields are present;
- unknown fields are rejected for format version 1 so they cannot be silently lost;
- IDs and relationship target types are valid;
- filenames match IDs;
- titles are non-empty;
- enum values are known;
- referenced records exist;
- spec paths stay inside the worktree and resolve to regular `.md` files;
- promotion, hierarchy, and release relationships are consistent in both directions;
- existing IDs were not removed or changed;
- parents belong to the same milestone;
- parent and dependency graphs are acyclic; and
- lifecycle invariants hold.

Validation is all-or-nothing. An invalid snapshot does not mutate SQLite or rewrite snapshot files.

## Snapshot synchronization

Arc Lightning compares three states before ordinary commands when snapshots are enabled:

- `B`: exact files stored in `snapshot_base`, the last common state;
- `D`: the current semantic database projection; and
- `S`: the current parsed snapshot projection.

The synchronization state machine is:

| Condition                        | Action                                                                                |
| -------------------------------- | ------------------------------------------------------------------------------------- |
| `D = B` and `S = B`              | Continue; nothing changed.                                                            |
| `D = B` and `S != B`             | Validate and import `S` transactionally, then set the new base.                       |
| `D != B` and `S = B`             | Export `D`, then set the new base. This also recovers from a crash after a DB commit. |
| `D = S` and both differ from `B` | Accept the shared state and update the base.                                          |
| `D != B`, `S != B`, and `D != S` | Report a synchronization conflict and perform no automatic write.                     |

This comparison is semantic for records and exact for the stored base files.

Front-matter field order or harmless whitespace may be normalized after import without creating a false logical conflict.

All successful mutating commands must:

1. synchronize and validate the starting state;
2. apply the database mutation in a transaction;
3. commit the database transaction;
4. export changed snapshot records through temporary files and atomic per-file renames;
5. update `snapshot_base` only after export succeeds; and
6. report any incomplete export as a recoverable synchronization state on the next invocation.

Before replacing a snapshot file, the exporter verifies that its current content still matches the content observed at command start.

If it changed concurrently, export stops and reports a conflict.

`arcl snapshot status`, `arcl snapshot import`, `arcl snapshot export`, and `arcl snapshot reconcile` inspect or
resolve the unsynchronized state directly and therefore bypass automatic reconciliation.

`arcl check` validates both sides and reports divergence without choosing a side.

All other commands pass through the automatic synchronization gate before reading or mutating domain data.

### Explicit synchronization commands

```text
arcl snapshot status
arcl snapshot export [--force]
arcl snapshot import [--force]
arcl snapshot reconcile --use database|snapshot [--force]
```

`status` is read-only and reports changed records on each side.
Forced import or export is destructive to one side, so it must show a summary and request confirmation on an interactive terminal.
Non-interactive callers must pass `--force`. Reconciliation never performs a Git operation.

## Project configuration

`.arcl/config.toml` uses this v1 shape:

```toml
format-version = 1

[snapshot]
enabled = true
path = ".arcl/snapshot"
```

Paths are relative to the worktree root. Unknown configuration keys produce an error in v1.

`.arcl/.gitignore` contains:

```gitignore
/arcl.db
/arcl.db-*
/*.tmp
/conflicts/
```

Arc Lightning creates this scoped ignore file instead of modifying the repository's root `.gitignore`.

## Git integration

v1 supports Git through `git-oxide/gix`. Arc Lightning must not shell out to `git`.

Define a narrow, read-only VCS boundary with the operations needed by the application:

```rust
trait Vcs {
    fn worktree_root(&self) -> Result<&Path, VcsError>;
    fn head_id(&self) -> Result<Option<String>, VcsError>;
    fn branch_name(&self) -> Result<Option<String>, VcsError>;
    fn path_state(&self, path: &Path) -> Result<PathState, VcsError>;
}
```

`GixVcs` is the only v1 implementation. Exact return ownership may change during implementation,
but the boundary must remain small and must not expose `gix` types to domain or storage modules.

Required behavior:

- discover the enclosing worktree from any descendant directory;
- reject operation outside a Git worktree with a helpful error;
- reject bare repositories;
- support unborn `HEAD` and detached `HEAD` without panicking;
- report whether configuration and snapshot paths are tracked, untracked, modified, or conflicted;
- identify unresolved Git conflict markers or unmerged snapshot paths before parsing; and
- never stage, commit, switch, merge, push, fetch, or modify Git configuration.

The VCS boundary keeps `gix` types out of application and domain code and allows Git states to be tested without a real repository.

## Plan application

Arc Lightning checks, diffs, and applies a complete plan for an existing epic:

```text
arcl plan check <epic-id> <path|->
arcl plan diff <epic-id> <path|->
arcl plan apply <epic-id> <path|->
```

The input is TOML and may come from a file or standard input.
It declares milestones, tasks, subtasks, and dependencies using stable plan-local keys.

```toml
format-version = 1

[[milestones]]
key = "foundation"
title = "Foundation"
position = 10

[[milestones.tasks]]
key = "storage"
title = "Implement SQLite storage"
priority = "high"
position = 10

[[milestones.tasks.subtasks]]
key = "migrations"
title = "Add embedded migrations"
position = 10

[[milestones.tasks]]
key = "ready-query"
title = "Implement ready-work query"
priority = "normal"
position = 20
blocked-by = ["foundation/storage"]
```

Plan application is transactional and additive:

- it validates the entire proposed graph before writing;
- it creates ULIDs for previously unseen plan keys;
- repeating the same plan updates matching records rather than duplicating them;
- omitted existing records remain unchanged;
- it does not complete, cancel, move, or delete omitted work;
- `check` validates the complete resulting graph and reports errors without writing;
- `diff` emits the deterministic creates and updates without writing; and
- `apply` writes all validated changes in one transaction.

Omitted records are never pruned.

Milestone `plan_key` values are unique within an epic. Task and subtask `plan_key` values are unique within a milestone.
A dependency reference uses `<milestone-key>/<task-key>` with further `/subtask-key` segments as needed, or a full existing `arcl-t` ID.

## Agent context and continuity

`arcl explain <task-id>` reports whether a task is ready and lists every condition that prevents it from being ready.
The explanation covers task state, children, ancestors, containers, active blockers, and malformed relationships.

`arcl context <task-id>` returns one bounded context packet containing:

- the task and its current lifecycle state;
- its parent task, milestone, epic, release, and linked spec path;
- direct blockers and dependents with their states;
- derived readiness and every reason the task is not ready;
- the current handoff note; and
- completion evidence from completed direct blockers.

The context packet is derived from existing records. It does not copy spec contents or create a stored summary.
Human and JSON output contain the same facts, and ordering is deterministic.

`arcl task handoff <id> --note <markdown>` accepts an `in_progress` task, stores the current handoff note,
and parks the task in one transaction. `--note-file <path|->` provides the same UTF-8 input behavior as descriptions.
Unparking retains the note until another handoff replaces it.

`arcl task complete <id> --evidence <markdown>` stores optional completion evidence in the same transaction as completion.
`--evidence-file <path|->` reads UTF-8 from a path or standard input. Evidence is descriptive provenance;
Arc Lightning does not execute commands, open URLs, or decide whether the evidence proves correctness.

## CLI contract

The command structure follows a consistent noun-and-verb hierarchy.

### Global commands

```text
arcl init [--snapshot]
arcl status
arcl check
arcl show <id>
arcl list [filters]
arcl ready [filters]
arcl next [filters]
arcl tree [<id>]
arcl explain <task-id>
arcl context <task-id>
arcl plan check <epic-id> <path|->
arcl plan diff <epic-id> <path|->
arcl plan apply <epic-id> <path|->
```

### Entity commands

```text
arcl idea create <title> [description options]
arcl idea update <id> [fields]
arcl idea discard <id>
arcl idea promote <id> --spec <path> [--release <id>]

arcl release create <title> [description options]
arcl release update <id> [fields]
arcl release complete <id> [--allow-open-children]
arcl release cancel <id> [--allow-open-children]

arcl epic create <title> --spec <path> [--release <id>] [description options]
arcl epic update <id> [fields]
arcl epic complete <id> [--allow-open-children]
arcl epic cancel <id> [--allow-open-children]

arcl milestone create <title> --epic <id> [--position <n>] [description options]
arcl milestone update <id> [fields]
arcl milestone complete <id> [--allow-open-children]
arcl milestone cancel <id> [--allow-open-children]

arcl task create <title> --milestone <id> [--parent <id>] [--blocked-by <id>...] [fields]
arcl task update <id> [fields]
arcl task start <id>
arcl task park <id>
arcl task unpark <id>
arcl task handoff <id> --note <markdown>|--note-file <path|->
arcl task complete <id> [--allow-open-children] [--evidence <markdown>|--evidence-file <path|->]
arcl task cancel <id> [--allow-open-children]

arcl dependency add <task-id> --blocked-by <task-id>
arcl dependency remove <task-id> --blocked-by <task-id>
```

Arc Lightning has no hard deletion or terminal-state reopening. Status transitions preserve completed and cancelled records.

Update commands expose only fields meaningful to that entity:

- ideas: title and description;
- releases: title and description;
- epics: title, description, spec path, and release association;
- milestones: title, description, and position; and
- tasks: title, description, priority, position, milestone, and parent.

Moving a task or changing its parent validates the entire moved subtree, requires all descendants
to remain in one milestone, and rejects any resulting parent or dependency cycle.

### Description input

Commands accepting descriptions support two mutually exclusive options:

```text
-d, --description <markdown>
    --description-file <path|->
```

`-` reads UTF-8 from standard input. A command that expects piped input must not wait silently when stdin is a terminal.

### Filters

`list`, `ready`, and `next` support applicable combinations of:

```text
--kind <idea|release|epic|milestone|task>
--status <value>...
--priority <value>...
--release <id>
--epic <id>
--milestone <id>
--parent <id>
```

Invalid filter combinations fail with an explanation and a corrected example.

### Output

- Default output is concise and designed for a terminal.
- `--json` writes versioned structured JSON to stdout.
- `--plain` writes one stable, unstyled record per line for shell pipelines.
- `--quiet` suppresses explanatory mutation output and prints only the affected ID.
- `--json`, `--plain`, and `--quiet` are mutually exclusive where their semantics conflict.
- Primary results go to stdout. Diagnostics, warnings, progress, and errors go to stderr.
- Broken-pipe errors terminate quietly with a successful or platform-conventional pipe status.
- No v1 command invokes a pager.

JSON objects include `format_version = 1` at the top-level response envelope.

IDs, enum values, and field names are stable public output contracts within v1.

### Color

Use `owo-colors` with its `supports-colors` feature. Apply styles only through terminal-support-aware formatting.

The global flag is:

```text
--color <auto|always|never>
```

The default is `auto`. `NO_COLOR` disables color and `FORCE_COLOR` enables it when no explicit flag is present.
An explicit flag wins over environment variables. JSON, plain, quiet, and non-terminal output never contain ANSI
escapes unless the caller explicitly requests `--color always` for human output.

### Help and errors

- `arcl`, `arcl --help`, `arcl help`, and help on every subcommand must work.
- Help leads with common examples and lists common commands before advanced commands.
- Unknown commands and invalid values include a likely correction when one is unambiguous.
- Errors identify the record and field involved and suggest a recovery command when possible.
- Expected user errors never print a Rust backtrace by default.

Exit codes are:

| Code | Meaning                                      |
| ---- | -------------------------------------------- |
| `0`  | success                                      |
| `1`  | unexpected application or I/O failure        |
| `2`  | CLI usage error, managed by `clap`           |
| `3`  | invalid project, record, plan, or graph      |
| `4`  | snapshot or concurrent-modification conflict |
| `5`  | requested record not found                   |

When `--json` is active, failures write a structured error object to stderr with `format_version`,
`code`, `message`, and optional `details` and `suggestion` fields.

## Rust architecture

v1 is a synchronous, single-crate application. Do not add Tokio or another async runtime.

Local Git inspection, SQLite, and filesystem snapshot operations are blocking.
Keeping the core synchronous preserves simple transaction and cancellation semantics.

Recommended module boundaries:

```text
src/
├── main.rs          # parse, invoke, render final error
├── lib.rs           # crate modules and application entry point
├── cli.rs           # clap types only
├── app.rs           # command orchestration and synchronization gate
├── output.rs        # human, plain, and JSON renderers
├── domain/          # IDs, entities, lifecycle, graph rules
├── storage/         # rusqlite connection, migrations, repositories
├── snapshot/        # codec, validation, comparison, import/export
├── plan/            # plan parser, validation, additive application
└── vcs/             # narrow VCS boundary and gix implementation
```

Rules:

- Domain modules do not depend on `clap`, `gix`, `rusqlite`, terminal styling, environment variables,
  or filesystem paths beyond validated spec path values.
- `rusqlite::Connection` has one clear owner per command and is never shared concurrently.
- Mutations are application services that coordinate validation, a SQLite transaction, and optional snapshot export.
- Use concrete structs by default. The VCS boundary is a trait because it isolates `gix` and provides a focused test seam.
- Use typed IDs and enums instead of raw strings in application and domain code.
- Production code must not use `unwrap`, `expect`, `panic!`, `todo!`, or `unimplemented!`
  for recoverable input, I/O, Git, SQLite, or snapshot failures.
- `thiserror` defines typed domain and infrastructure errors.
- `anyhow` is restricted to the binary/application boundary for contextual propagation and final reporting.
- No dependency types appear in the public JSON schema.

## Dependencies

Use compatible releases from these lines and commit `Cargo.lock`:

| Crate        | Version line | Required features or purpose                                                     |
| ------------ | ------------ | -------------------------------------------------------------------------------- |
| `clap`       | `4.6`        | `derive`; CLI parsing and help                                                   |
| `gix`        | `0.85`       | minimal local discovery, index, status, and worktree features; no network client |
| `owo-colors` | `4.3`        | `supports-colors`                                                                |
| `rusqlite`   | `0.39`       | `bundled` for consistent standalone SQLite on Rust 1.88                         |
| `serde`      | `1`          | `derive`                                                                         |
| `toml`       | `1.1`        | config, front matter, and plan parsing                                           |
| `ulid`       | `1.2`        | `serde`                                                                          |
| `thiserror`  | `2`          | typed errors                                                                     |
| `anyhow`     | `1`          | application boundary only                                                        |

Do not enable `gix` default or network features without checking the resolved feature tree.
Record the minimal chosen feature set in `Cargo.toml` comments or architecture documentation because `gix` features
materially affect build time and transitive dependencies.

Test-only crates such as `assert_cmd`, `assert_fs` or `tempfile`, and `predicates` may be added when they reduce custom harness code.
Keep the set minimal.

## Testing plan

### Test boundary

The highest stable boundary is the compiled `arcl` binary operating in temporary Git worktrees.
End-to-end tests must assert exit codes, stdout, stderr, database state, and snapshot files.
Unit tests cover pure domain and codec behavior where CLI tests would obscure the invariant.

No test may depend on the user's global Git configuration, current repository, home directory,
locale-specific formatting, or wall-clock sleeps.

### Unit tests

Cover:

- every typed ID prefix and malformed ULID case;
- lifecycle transitions and rejected transitions;
- readiness ordering and each exclusion condition;
- readiness explanations containing every applicable exclusion reason;
- cancelled blockers remaining unsatisfied;
- task parent and dependency cycle detection;
- same-milestone parent enforcement;
- container completion guards;
- idea-promotion idempotency;
- TOML front-matter parsing and canonical rendering;
- Markdown body round trips, including empty and Unicode bodies;
- path normalization and worktree escape attempts;
- deterministic context-packet assembly;
- handoff-and-park and evidence-and-complete atomicity;
- plan-local key resolution, check and diff output, and idempotent plan application; and
- three-way synchronization state selection.

### SQLite tests

Use a temporary database for:

- migration from an empty database;
- foreign-key enforcement;
- rollback after each validation or write failure;
- busy-timeout behavior without indefinite blocking;
- uniqueness of spec paths and plan keys;
- atomic idea promotion;
- deterministic ready queries;
- atomic handoff and completion-evidence updates; and
- rebuilding the database from a complete snapshot.

### CLI and Git tests

Create isolated temporary repositories and cover:

- initialization at the root and from a nested directory;
- operation outside Git and in a bare repository;
- unborn and detached `HEAD`;
- database ignore behavior without root `.gitignore` edits;
- default, plain, quiet, and JSON output;
- explain and context output for ready and blocked tasks;
- handoff persistence through park and unpark transitions;
- completion evidence preserved through snapshot round trips;
- no ANSI output under `NO_COLOR` or non-TTY output;
- external snapshot edits imported on the next command;
- a branch checkout replacing snapshot content and rebuilding local state;
- independent record changes remaining isolated to independent files;
- malformed TOML and unresolved merge conflicts;
- database-only crash recovery;
- dual-side synchronization conflicts refusing writes;
- explicit forced reconciliation;
- plan checks, deterministic diffs, and repeated plan application; and
- verification that no command changes Git index, refs, remotes, or configuration.

### Required commands

The implementation is not ready until these pass:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo doc --workspace --all-features --no-deps
```

Human review must also run the documented capture, promotion, planning, ready-work, snapshot-edit,
branch-switch, and conflict-recovery workflows in a disposable repository.

## Implementation phases

These phases define verification boundaries. `TODO.md` will later split them into implementation tickets and dependencies.

### Foundation

Define domain types, synchronous application boundaries, CLI shell, SQLite connection policy, and embedded schema migrations.

Evidence: typed-ID, lifecycle, migration, and basic create/show CLI tests pass.

### Work graph

Implement releases, epics, milestones, tasks, subtasks, dependencies, lifecycle transitions, hierarchy views,
ready-work explanations, task context packets, handoffs, and completion evidence.

Evidence: complete hierarchy, readiness, context, handoff, and completion workflows pass through the compiled binary.

### Snapshot collaboration

Implement the TOML-plus-Markdown codec, full-graph validation, base-state storage, three-way synchronization,
atomic file replacement, and recovery commands.

Evidence: manual edits, branch switches, crash states, and dual-side conflicts pass end-to-end tests without silent overwrite.

### Planning workflow

Implement ideas, idempotent promotion, release association, and transactional plan checking, diffing, and application.

Evidence: one idea can be promoted and planned into milestones, tasks, subtasks, and dependencies without duplicate records on retry.

### CLI hardening

Complete Git status integration, human and machine output, color policy, help, diagnostics,
exit codes, documentation, and cross-platform verification.

Evidence: all required commands pass, CLI behavior matches this specification, and
a human can complete the v1 workflow using only terminal help.

## Boundaries

### Always

- Preserve user-authored spec and snapshot content.
- Validate the entire affected graph before mutation.
- Use transactions for related database changes.
- Refuse ambiguous synchronization.
- Keep Git access read-only.
- Add regression tests for every corrected data-loss, graph-integrity, or synchronization bug.
- Update this specification when implementation reveals a required contract change.

### Ask first

- Add a production dependency not listed in this specification.
- Change an ID, snapshot, plan, config, or JSON output format.
- Change lifecycle semantics or readiness rules.
- Introduce hard deletion, automatic cascading transitions, or snapshot pruning.
- Expand the VCS interface or enable `gix` network behavior.
- Introduce async execution, background processes, hooks, or long-running services.

### Never

- Commit the SQLite database.
- Silently overwrite diverged database or snapshot state.
- Execute Git mutations on the user's behalf.
- Modify a linked spec as a side effect of tracker maintenance.
- Ignore unknown snapshot fields and then rewrite the record.
- Treat malformed or missing relationships as ready work.
- Store secrets or credentials in tracker configuration or snapshots.

## Risks and review points

### Snapshot and database atomicity

SQLite and multiple snapshot files cannot share one filesystem transaction.
The stored common base, optimistic pre-write comparison, temporary files, and explicit conflict recovery are required to prevent silent loss.
This is the highest-risk implementation area and needs failure-injection tests.

### Manual snapshot editing

Human-editable records improve interoperability but allow inconsistent graphs.
Import must remain full-graph, strict, and transactional.
Error messages should identify the file, field, and relationship path.

### Git implementation complexity

`gix` has a large feature surface. v1 must select only local features needed for discovery and status
and must test behavior in worktrees, detached states, and untrusted repositories.

### File volume

One file per record favors diffs and merges at the cost of repository file count. v1 accepts that tradeoff.
Performance work should follow measurements from real repositories rather than introduce a second snapshot layout.

### SQLite concurrency

SQLite remains single-writer. A busy timeout prevents instant transient failures,
while short transactions and conflict reporting prevent indefinite waits.

## References

- Command Line Interface Guidelines: <https://clig.dev/>
- `gix`: <https://docs.rs/gix/>
- `owo-colors`: <https://docs.rs/owo-colors/>
- `rusqlite`: <https://docs.rs/rusqlite/>
- `ulid-rs`: <https://github.com/dylanhart/ulid-rs>
- Beads: <https://github.com/gastownhall/beads>
- Beans: <https://github.com/hmans/beans>
- Chainlink: <https://github.com/dollspace-gay/chainlink>
