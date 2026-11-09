---
title: Manual
description: Every arcl command and option, with output formats, exit codes and the rules each command enforces.
---

`arcl` operates on the enclosing non-bare Git worktree. Run `arcl --help` or
`arcl <command> --help` for the generated command help.

## Options

These options can appear before or after a subcommand & are global:

| Option                        | Purpose                                                                          |
| ----------------------------- | -------------------------------------------------------------------------------- |
| `--color auto\|always\|never` | Choose when human output may use ANSI colors. Defaults to `auto`.                |
| `--json`                      | Emit a versioned JSON envelope. Mutually exclusive with `--plain` and `--quiet`. |
| `--plain`                     | Emit one stable, unstyled record per line.                                       |
| `--quiet`                     | Print nothing on success. Errors still go to standard error.                     |
| `--help`                      | Print help for the current command.                                              |
| `--version`                   | Print the installed version.                                                     |

## Record IDs

A full record ID is `arcl-<kind>-<ULID>`, such as
`arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`. Every command that takes a record ID also
takes a short ID: the end of the ULID, at least 4 characters, such as `GBA5`.
Put the kind and a dash in front to match only that kind: `t-GBA5`. The kinds
are `c`, `r`, `s`, `pl`, `ph`, `t`, `n` and `g`, for gates. Letters match in
either case.

A command that expects one kind, such as `task start` or `--spec`, matches a
short ID without a kind only against records of that kind.

A short ID must end exactly one record's ID. One that ends several fails with
status 3 and lists each match's full ID and title; one that ends none fails with
status 5.

Records made close together share the start of their IDs, so a short ID is
always the end, never the start. Input that matches the start of any record's
ULID, such as `01M5`, fails with status 3 and lists matching records with the
last characters to type instead. A whole ULID still resolves.

Human output from `show`, `list`, `explain`, `task show` and `task explain`,
and the blockers `tree` names, prints each ID, readiness reasons included, as
its shortest suffix that ends no other record's ID and starts none, at least 4
characters.
Records of different kinds that share a whole ULID, as a copied workspace file
can, print as `<kind>-<ULID>`, such as `t-01M5VDDD42HZQ1FEN49X80GBA5`.
`--plain` and `--json` output keep full IDs.

A short ID that is unique today can match a second record later. Write full IDs
in files, evidence, links and scripts that outlive the command.

<div id="project-initialization"></div>

## Project setup

### `arcl init`

Initialize or rediscover the project in the enclosing Git worktree.

```sh
arcl init
arcl init --workspace
```

The command creates `.arcl/config.toml`, `.arcl/arcl.db`, and a scoped
`.arcl/.gitignore`. It rejects directories outside Git worktrees and bare
repositories. Re-running initialization preserves existing project state.

A fresh clone or worktree has the committed config and workspace but no
database. Other commands fail there with exit code `3` and say to run
`arcl init`; the command after it imports the workspace.

Every command that opens the database applies the migrations an older
database needs. A database written by a newer arcl is refused before any
read or write, with exit code `3`:

```text
database schema version 9 was written by a newer arcl; this arcl supports schema versions up to 8. Upgrade arcl to open it
```

The CLI, `arcl serve` and the `arcl-mcp` server all run this check, and
leave the database as it was. A workspace whose `manifest.toml` has a newer
`format-version` is refused the same way before any workspace file is read
or written:

```text
workspace format version 7 was written by a newer arcl; this arcl supports workspace format versions up to 6. Upgrade arcl to use this workspace
```

arcl 0.3.0 reads only workspace format 1, and refuses any later version with
`unsupported configuration format version 6; expected 1`. The first
`task complete` in a workspace writes format 6, since the task file gains
`completed-at`. After that, teammates and CI on arcl 0.3.0 cannot read the
workspace until they upgrade to the release after 0.3.0.

### `arcl skill install`

Install the Arc Lightning agent skill into the project.

```sh
arcl skill install
arcl skill install --check
```

The command writes the skill into `.claude/skills/arclightning/` and
`.agents/skills/arclightning/` under the project root, and leaves identical
files unchanged. The files are built into `arcl` from the repository's
`skills/arclightning/` directory, so the installed skill matches the binary
that wrote it.

`--check` writes nothing. It reports each file as `unchanged`, `missing`, or
`changed`, and exits with status code `4` when any file is missing or changed.
Without `--check`, each file is `created`, `updated`, or `unchanged`. `--plain`
prints `<status>\t<path>` per file, and `--json` prints `action` (`installed`
or `checked`), `root`, and a `files` array of `path` and `status`.

The command checks every target before it writes any file, and replaces a file
by writing a temporary file beside it and renaming it. It exits with status
code `3` and writes nothing when a skill file is a symbolic link or not a
regular file, a skill directory is not a directory, or a path resolves outside
the project root. A linked directory, such as `.agents/skills` pointing at
`../.claude/skills`, is followed: the file is written once and its second path
is reported as `unchanged`.

### `arcl setup`

Write the files a coding agent needs to use arcl: the skill, the arcl lines in
its instruction file, and, where the agent can run one, a hook that runs
`arcl prime` when a session starts. With no agent named, `arcl setup` runs
`arcl setup other`.

```sh
arcl setup
arcl setup claude
arcl setup codex --dry-run
arcl setup pi --yes
```

| Agent    | Instruction file | Hook                                                        |
| -------- | ---------------- | ----------------------------------------------------------- |
| `claude` | `CLAUDE.md`      | `SessionStart` command in `.claude/settings.json`           |
| `codex`  | `AGENTS.md`      | `SessionStart` command in `.codex/hooks.json`               |
| `pi`     | `AGENTS.md`      | `session_start` extension in `.pi/extensions/arcl-prime.ts` |
| `other`  | `AGENTS.md`      | None                                                        |

Every agent gets the skill as `arcl skill install` writes it, in
`.claude/skills/` and `.agents/skills/`. `other` is for agents that read
`AGENTS.md` and `.agents/skills/`; it writes no hook, so have the agent run
`arcl prime` when a session starts. The instruction lines sit between
`<!-- arcl-setup:begin -->` and `<!-- arcl-setup:end -->`, each on a line of
its own. A marker inside a line of text does not count. A later run replaces
that block and leaves the rest of the file alone.

The hooks run `if command -v arcl >/dev/null; then arcl prime; fi`. A settings
file whose `SessionStart` hooks already run exactly that command, with no
matcher or a matcher that covers `startup`, is left as it is. Otherwise the
hook is added and every other byte of the file stays as it was, apart from the
line breaks a one-line root object, `hooks` or `SessionStart` gains to hold
it. New lines take the file's indentation and line ending, and a new settings
file is indented by two spaces. Both settings files must be strict JSON: a
comment, a trailing comma or other JSONC syntax exits with status code `3`,
since Claude Code and Codex read them as JSON. Pi has no command hooks, so it
gets an extension that runs `arcl prime` on `session_start` and adds the
output to the next prompt. A symbolic link, such as `CLAUDE.md` pointing at
`AGENTS.md`, is followed to its file under the project root.

A missing file is created. Before changing a file that exists, the command
prints the diff to standard error and asks for `y`. `--yes` skips the
question. With no terminal on standard input and no `--yes`, it writes nothing
and exits with status code `4`, naming the files. Any answer other than `y` or
`yes` writes nothing and exits with status code `1`. If a file changes between
the diff and the write, such as an edit saved while the question waits, the
command writes nothing and exits with status code `4`, naming the file.

`--dry-run` writes nothing. Its human and `--json` output include each change as
a unified diff. Each file is `created`, `updated`, or `unchanged`, and a second
run reports every file as `unchanged` and writes nothing. `--plain` prints
`<status>\t<path>` per file. `--json` prints `action` (`applied`, or `planned`
under `--dry-run`), `agent`, `agent_name`, `root`, `notes`, and a `files` array
of `path`, `status` and, under `--dry-run`, `diff`. A settings file that is not
strict JSON, is not a JSON object, or repeats a key on the path to the hook,
such as two `hooks` keys, exits with status code `3`. So does an instruction
file with a begin marker and no end marker, or with two begin markers.

Codex runs project hooks only in a trusted project, after you trust the hook
in `/hooks`. Pi loads `.pi/extensions/` and `.agents/skills/` only after you
trust the project. `other` notes that it wrote no start-up hook. The command
prints these as notes.

## Workspace

| Command                                          | Purpose                                                         |
| ------------------------------------------------ | --------------------------------------------------------------- |
| `arcl workspace export`                          | Write the local database state to the configured workspace.     |
| `arcl workspace import`                          | Validate the workspace and rebuild the local database.          |
| `arcl workspace sync`                            | Carry changes made on one side to the other.                    |
| `arcl workspace status`                          | Show what changed since the last sync, without writing.         |
| `arcl workspace resolve --keep <side> <path>...` | Keep one side's version of files both sides changed, then sync. |

Workspace commands require the workspace to be enabled with
`arcl init --workspace`. A concurrent file change, or a file the database and
the workspace changed differently, stops the command with status code `4`
instead of overwriting either version.

See [Share the project workspace](/guides/workspace/) for the Git workflow and
workspace layout.

<div id="idea-commands"></div>

## Capture commands

| Command                                        | Purpose                                         |
| ---------------------------------------------- | ----------------------------------------------- |
| `arcl capture create <title>`                  | File a capture in the inbox.                    |
| `arcl capture show <id>`                       | Show one capture.                               |
| `arcl capture list`                            | List every capture in creation order.           |
| `arcl capture update <id>`                     | Replace the title or body.                      |
| `arcl capture discard <id>`                    | Mark the capture `discarded`, keeping the file. |
| `arcl capture promote <id> <spec\|task\|note>` | Create a spec, task or note from the capture.   |

Bodies use `--body <MARKDOWN>` (`-b`) or `--body-file <PATH|->`, here and in
every command that takes a Markdown body. The two are mutually exclusive, and
`-` reads UTF-8 Markdown from standard input.

`arcl capture create <title>` files a capture in the Inbox. `--from <task>`
records the task it came from, by full or short ID. Without `--from`, the
capture takes the one in-progress task claimed by `ARCL_AGENT`. With no such
task it has no source; with several it has none and the command warns,
naming them. An unknown task fails with status code `5`, and an ID that is
not a task's with `3`. Only the CLI fills in the source: the MCP
`capture_create` tool records one only when the call passes `from`.

`show` prints the source as a `From: task` line, `--plain` show as a `from`
line, and `--json` as `source_task_id`, which it leaves out when there is no
source. A promoted spec, task or note keeps it: `show`, `spec show`,
`task show` and `note show` on the new record read it through the capture's
promotion, and a capture file stores it as `source-task`.

`promote` takes the target as a positional argument or `--to <KIND>`.
`--title`, `--body` and `--body-file` override what the new record takes from
the capture. A spec takes `--acceptance-criteria` or
`--acceptance-criteria-file`. A task takes `--spec`, `--plan`, `--phase`,
`--parent`, `--priority` (default `normal`) and `--position` (default `0`).
Promotion creates the record, links it and marks the capture `promoted` in
one transaction. Repeating it with the same target returns the existing
record; another target, or a discarded capture, fails with exit code `3`. A
discarded capture cannot be updated. See [Captures](/guides/captures/).

<div id="epics"></div>

## Spec commands

| Command                    | Purpose                              |
| -------------------------- | ------------------------------------ |
| `arcl spec create <title>` | Create an open spec.                 |
| `arcl spec show <id>`      | Show one spec.                       |
| `arcl spec list`           | List specs.                          |
| `arcl spec update <id>`    | Replace the title, body or criteria. |
| `arcl spec complete <id>`  | Complete a spec.                     |
| `arcl spec cancel <id>`    | Cancel a spec.                       |

`create` and `update` take `--acceptance-criteria <MARKDOWN>` or
`--acceptance-criteria-file <PATH|->`, and `update` takes `--title`. See
[Specs](/guides/specs/).

## Plan commands

| Command                                 | Purpose                                                  |
| --------------------------------------- | -------------------------------------------------------- |
| `arcl plan create <title> --spec <ID>`  | Create an open plan under a spec.                        |
| `arcl plan show <id>`                   | Show a plan with its phase, task and dependency counts.  |
| `arcl plan list`                        | List plans.                                              |
| `arcl plan update <id>`                 | Replace the title or body.                               |
| `arcl plan check <id> --file <PATH\|->` | Validate a plan file and list its changes, writing none. |
| `arcl plan diff <id> --file <PATH\|->`  | List the changes a plan file would make.                 |
| `arcl plan apply <id> --file <PATH\|->` | Apply a plan file in one transaction.                    |
| `arcl plan complete <id>`               | Complete a plan.                                         |
| `arcl plan cancel <id>`                 | Cancel a plan.                                           |

`create --input <PATH|->` applies a plan file after creating the plan, in the
same command. `--no-input`, the default, creates the plan alone; the two
conflict. `check` and `diff` print one tab-separated line per phase, task
and dependency: `phase` or `task` with its key path, or `dependency` with the
task's path and the blocker's, then the change. The modes differ:

- Human output opens with a ``Plan `<id>` `` line and writes the change as
  `Create`, `Update` or `Unchanged`.
- `--plain` has no header line and writes `create`, `update` or `unchanged`.
- `--json` returns `plan_id` and `phases`, `tasks` and `dependencies` lists.
  A phase or task entry has `key`, `id` (`null` for `create`) and `change`; a
  dependency entry has `task`, `blocker` and `change`. `change` is
  lowercase, as in `--plain`.

A plan file that does not parse, or names an unknown or ambiguous dependency,
fails with exit code `3`. [Plans](/guides/plans/) describes the file format.

<div id="milestones"></div>

### Phase commands

| Command                                      | Purpose                              |
| -------------------------------------------- | ------------------------------------ |
| `arcl plan phase create <title> --plan <ID>` | Create an open phase in a plan.      |
| `arcl plan phase show <id>`                  | Show one phase.                      |
| `arcl plan phase list`                       | List phases.                         |
| `arcl plan phase update <id>`                | Replace the title, body or position. |
| `arcl plan phase complete <id>`              | Complete a phase.                    |
| `arcl plan phase cancel <id>`                | Cancel a phase.                      |

`--position <N>` orders phases in their plan; `create` defaults it to `0`.
See [Phases](/guides/phases/).

<div id="task-management"></div>

## Task commands

| Command                      | Purpose                                                 |
| ---------------------------- | ------------------------------------------------------- |
| `arcl task create <title>`   | Create a pending task or subtask.                       |
| `arcl task show <id>`        | Show a task with readiness, ancestry and blockers.      |
| `arcl task list [--all]`     | List tasks, leaving out archived ones without `--all`.  |
| `arcl task update <id>`      | Replace metadata, move a subtree, or change its parent. |
| `arcl task start <id>`       | Move ready pending work to `in_progress`.               |
| `arcl task park <id>`        | Move pending or in-progress work to `parked`.           |
| `arcl task unpark <id>`      | Return parked work to `pending`.                        |
| `arcl task handoff <id>`     | Store a resume note and park in-progress work.          |
| `arcl task note <id> <text>` | Append a timestamped note, with an optional `--kind`.   |
| `arcl task complete <id>`    | Mark pending or in-progress work completed.             |
| `arcl task cancel <id>`      | Cancel work that is not completed or cancelled.         |
| `arcl task explain <id>`     | The same report as `arcl explain`.                      |
| `arcl task context <id>`     | The same packet as `arcl context`.                      |

Task creation accepts `--spec <ID>`, `--plan <ID>`, `--phase <ID>`,
`--parent <ID>`, `--priority <critical|high|normal|low>` (default `normal`),
`--position <N>` (default `0`), and `--blocked-by <ID>`. A phase places the
task under its plan and spec, and a plan under its spec. A subtask takes its
spec, plan and phase from its parent; naming a different one fails with exit
code `3`. The spec, plan or phase must be open. `--blocked-by` takes one ID
per flag; repeat the flag or separate IDs with commas to add several
blockers, as `dependency add` does.

Task updates accept `--title`, `--body`, `--body-file`, `--priority`,
`--position`, and `--spec`, `--plan`, `--phase` or `--parent` with their
`--no-spec`, `--no-plan`, `--no-phase` and `--no-parent` forms. Moving a task
moves its entire descendant subtree. A cycle, a move that leaves a task
blocked by its own ancestor or descendant, or a move under an archived task
is refused before storage changes.

`start` refuses a pending task that is not ready and lists the reasons. Having
child tasks is the one reason it skips. `--force` starts the task anyway.

`start --claim [<NAME>]` also sets the task's assignee in the same write. With
no name it uses `ARCL_AGENT`, then the Git user name. A name is at most 64
characters, with no control or format characters such as zero-width spaces or
direction marks. While a task has an assignee, a start without `--claim`, or
with another name, is refused with exit code `4` and the `conflict` error code;
`--force` does not override it. Parking, handing off, completing, and
cancelling clear the assignee, whoever runs them. A claim holds within one
database, which is one checkout's `.arcl/arcl.db`; agents in separate Git
worktrees see each other's claims only after the workspace change reaches their
checkout through Git and a sync.
`arcl list --assignee <NAME>` lists the tasks one assignee holds.

`handoff` requires either `--note <MARKDOWN>` or `--note-file <PATH|->`.

`note` appends a note to a task in any status, completed included. `--kind`
takes `decision`, `observation`, `blocker`, or `result`; without it the note has
no kind. The text is trimmed, at most 2000 characters, and may span lines. It is
refused with exit code `3` when it is empty or holds a control character other
than newline and tab, a format character, or a line that is exactly `+++`. arcl
commands only append notes; editing or deleting a note in the task file and
running `workspace sync` changes or removes it. `show` lists every note, newest
first, and `context` lists the 10 most recent and reports how many it left out:
`omitted_notes` in JSON and plain output, an `Older notes` line in human output.
In JSON a note has `task_id`, `created_at`, `kind` when set, and `text`;
`task note --json` returns `{action: "noted", note}`. Plain output prints
`note<TAB><created-at><TAB><kind><TAB><text>`, with an empty kind field when the
note has none.

Completion accepts either `--evidence <MARKDOWN>` or `--evidence-file <PATH|->`.
Each pair is mutually exclusive, and `-` reads UTF-8 Markdown from standard
input.

Completing a task records when, as `completed_at` in JSON and `completed-at` in
its workspace file. Subtasks that `--with-subtasks` completes get the same
time. A task completed before arcl recorded completion times has none.

Only arcl commands set `completed-at`. A task file whose `status` is set to
`completed` by hand imports with no completion time, since sync cannot tell
when the work was done or tell that file from one written before completion
times. `compact` skips such a task unless `--include-undated` is given. To
reopen a task by hand, set its `status` and delete `completed-at`, and
`archived-at` if it has one; sync refuses an open task that carries either.

`complete` refuses a task with open descendants. `--allow-open-children`
completes only the task. `--with-subtasks` also completes its pending and
in-progress descendants in one write, and refuses, writing nothing, while a
descendant is parked or a blocker outside the subtree is unfinished. The two
flags conflict. JSON output from `task complete` includes `completed_subtasks`,
the IDs it completed in tree order, and `unblocked`, the IDs of tasks that were
not ready before the command and are ready after it. Plain output prints the
task ID, then a `subtask<TAB><id>` line for each completed subtask and an
`unblocked<TAB><id>` line for each task it made ready.

## Dependency commands

| Command                                              | Purpose                                 |
| ---------------------------------------------------- | --------------------------------------- |
| `arcl dependency add <task-id> --blocked-by <ID>`    | Make a task wait for one or more tasks. |
| `arcl dependency remove <task-id> --blocked-by <ID>` | Remove one blocker.                     |
| `arcl dependency list`                               | List every dependency in the project.   |

`add` takes several blockers, by repeating the flag or separating IDs with
commas, and adds none if one is refused. It refuses an edge that closes a
cycle, joins a task to its own ancestor or descendant, or touches an archived
task, with exit code `3`. [Dependencies and ready work](/reference/deps/)
explains how blockers affect readiness.

## Compaction

`arcl compact --before <DATE>` archives tasks completed before the start of
`DATE` in UTC, written `YYYY-MM-DD`, and keeps their evidence in one note per
phase. `--phase <ID>` archives only tasks under that phase. `--dry-run` prints
what would be written and archived, and writes nothing.

A task is archived when all of these hold:

- it is completed, and not archived yet;
- its completion time is before `DATE`;
- it sits under a phase, its own or its parent's;
- it does not block a pending, in-progress or parked task, or a task with
  such a subtask;
- every subtask it has is archived in the same run or already.

Pending, in-progress, parked and cancelled tasks are never archived, so a
parent with a cancelled subtask stays. A task with no phase, on itself or an
ancestor task, is never archived, since its evidence has no phase note to go
to. A completed task with no completion
time stays unless `--include-undated` is given, which treats it as completed
before any date.

An archived task keeps its status, `completed`, and gets `archived_at` in JSON
and `archived-at` in its workspace file. It stays in the workspace files.

Each phase's tasks go into its compaction note, titled `Compacted work: <phase
title>` and linked to the phase. Its ID comes from the phase's ID, so every
clone writes the same `notes/<id>.md` file for a phase. Other notes linked to
the phase are left alone, whatever their title. The note holds a section per
task, oldest completion first, with its title as a heading, its ID, its
completion time and its evidence. A later run appends new sections to the same
note. A task reopened by hand and compacted again gets its section replaced in
place. Archived tasks are never selected again, so running the same command
twice changes nothing the second time.

Compact on one branch. Two clones that compact the same tasks set different
`archived-at` times, and two that compact different tasks of a phase write
different versions of its note, so merging them conflicts in git on those
files.

An archived task takes no new work. `task create` with `--parent` or
`--blocked-by`, `task update --parent`, `dependency add`, the MCP `task_create`
tool and a raw edit in the web app refuse a task under an archived task, and a
dependency to or on one, with exit code `3`:

```text
task `arcl-t-…` is archived; a task cannot be created or moved under an archived task
```

`gate add` already refuses a completed task.

`list`, `task list`, `tree` and `context` leave archived tasks out; `--all`
shows them. `show` always shows a task. The web app's Work list and board
leave them out too. `tree`, `roadmap` and the web app count archived
tasks as done in phase progress, so a phase keeps its totals after compaction.
`prime`, `ready` and `next` list only open tasks, so archived tasks never show
there.

Human output names each phase, its note and its tasks. Plain output prints a
`phase<TAB><phase-id><TAB><note-id><TAB><created|appended>` line per phase,
with `-` for the note a dry run would create, then a `task<TAB><id>` line per
task. JSON returns `dry_run`, `archived_at`, which is `null` on a dry run or
when nothing was archived, and `phases`, each with `phase_id`, `phase_title`,
`note_id`, `note_title`, `note_created` and `tasks`. A date that is not
`YYYY-MM-DD` exits `3`.

## Gates

A gate makes a task wait on something outside arcl. An open gate keeps its
task from being ready, and only `gate check` marks it met.

| Command                                  | Purpose                                                 |
| ---------------------------------------- | ------------------------------------------------------- |
| `arcl gate add <task-id> --pr <URL>`     | Wait for a pull request to merge.                       |
| `arcl gate add <task-id> --run <URL>`    | Wait for a GitHub Actions run to succeed.               |
| `arcl gate add <task-id> --until <DATE>` | Wait until a date, `YYYY-MM-DD`, arrives in UTC.        |
| `arcl gate list [<task-id>]`             | List gates on one task, or on every task, open and met. |
| `arcl gate remove <gate-id>`             | Remove a gate, open or met.                             |
| `arcl gate check [<task-id>]`            | Check open gates and mark the met ones.                 |

`gate add` takes exactly one of `--pr`, `--run` and `--until`. A pull request
URL is `https://<host>/<owner>/<repo>/pull/<number>` and a run URL
`https://<host>/<owner>/<repo>/actions/runs/<id>`; a trailing slash is
dropped and the host lowercased. A date must be a real calendar date. Other
targets, and a completed or cancelled task, are refused with exit code `3`.
Gate IDs are `arcl-g-<ULID>`, and take short IDs such as `g-535B`.

An open gate adds an `open_gate` readiness reason, such as
``gate `arcl-g-…` waits for pull request `https://…/pull/7` to merge``, and
sets `blocked`. In JSON the reason carries `gate` with the gate's `id`,
`kind`, `target` and `status`. `ready`, `next`, `next --start` and `start`
without `--force` all respect it. A gate holds back only its own task, not its
children.

`gate check` checks every open gate in scope:

| Kind   | Met when                                        | Asks                                                                 |
| ------ | ----------------------------------------------- | -------------------------------------------------------------------- |
| `date` | today's UTC date is on or after the gate's date | nothing; it needs no network                                         |
| `pr`   | the pull request merged                         | `gh pr view <url> --json state,mergedAt`                             |
| `run`  | the run completed with conclusion `success`     | `gh run view <id> --repo <host/owner/repo> --json status,conclusion` |

Each gate's outcome is `met`, `waiting`, `failed` (a pull request closed
without merging, or a run that finished without succeeding), `error` (`gh`
failed, ran past 30 seconds, or printed something unreadable) or `skipped`.
Only `met` changes the gate. A gate on a completed or cancelled task is
`skipped` with `task is completed` or `task is cancelled`, and is not checked.
`ARCL_GH_TIMEOUT_MS` sets the `gh` limit in milliseconds; `0` or a value that
is not a number keeps 30 seconds.
When `gh` is not on `PATH`, pull request and run gates are `skipped` and the
check reports `gh not found; PR/run gates left open`. One gate's error never
fails the check, which exits `0` unless the database fails. The check holds the
project's sync lock while it reads the gates and while it marks the met ones,
but not while `gh` runs.

Human output prints one line per checked gate, such as
``Met: gate `QYF6` on task `H3TN`: merged``, or `No open gates.`. Plain output
prints `<outcome><TAB><gate-id><TAB><task-id><TAB><detail>` per gate, then
`gh_missing` when it applies. JSON returns `{action: "checked", checks,
gh_missing}`, where each check has `gate`, `outcome` and `detail`. In JSON a
gate has `id`, `task_id`, `kind`, `target`, `status`, `created_at` and, once
met, `met_at`. `gate add` and `gate remove` return `{action, gate}` and
`gate list` returns `{gates}`. Plain output prints a gate as
`gate<TAB><id><TAB><task-id><TAB><kind><TAB><target><TAB><status><TAB><met-at>`,
which `show` and `context` also print for each gate on the task.

`arcl show <gate-id>` shows one gate. Human output prints the gate, its task
and when it was created. Plain output prints the gate's `gate` line, then
`created<TAB><created-at>`. JSON returns `{action: "shown", gate}`.

## Note commands

| Command                                                          | Purpose                    |
| ---------------------------------------------------------------- | -------------------------- |
| `arcl note create <title>`                                       | Create a note.             |
| `arcl note show <id>`                                            | Show one note.             |
| `arcl note list`                                                 | List notes.                |
| `arcl note update <id>`                                          | Replace the title or body. |
| `arcl note link add <note-id> --kind <KIND> --record-id <ID>`    | Link the note to a record. |
| `arcl note link remove <note-id> --kind <KIND> --record-id <ID>` | Remove one link.           |
| `arcl note link list <note-id>`                                  | List the note's links.     |

`--kind` is `capture`, `spec`, `plan`, `phase`, `task`, `note` or `release`,
and a short `--record-id` matches only records of that kind. Notes have no
status. See [Notes](/guides/notes/).

## Release commands

| Command                                                                  | Purpose                         |
| ------------------------------------------------------------------------ | ------------------------------- |
| `arcl release create <title>`                                            | Create an open release.         |
| `arcl release show <id>`                                                 | Show one release.               |
| `arcl release list`                                                      | List releases.                  |
| `arcl release update <id>`                                               | Replace the title or body.      |
| `arcl release complete <id>`                                             | Complete a release.             |
| `arcl release cancel <id>`                                               | Cancel a release.               |
| `arcl release member add <release-id> --kind <KIND> --record-id <ID>`    | Add a spec, plan, task or note. |
| `arcl release member remove <release-id> --kind <KIND> --record-id <ID>` | Remove one member.              |
| `arcl release member list <release-id>`                                  | List the explicit members.      |

`--kind` is `spec`, `plan`, `task` or `note`. Membership never includes a
record's descendants. See [Releases and roadmap](/guides/releases/).

## Inspection

| Command                                                              | Purpose                                                              |
| -------------------------------------------------------------------- | -------------------------------------------------------------------- |
| `arcl show <id>`                                                     | Show one record by its full or short ID.                             |
| `arcl list [--all]`                                                  | List records using kind, state, priority, and association filters.   |
| `arcl tree [<id>] [--all] [--ascii] [--depth <N>] [--style <STYLE>]` | Show the complete hierarchy or one rooted subtree.                   |
| `arcl roadmap [--output <PATH>]`                                     | Print specs, plans, phases and releases as one Markdown page.        |
| `arcl explain <task-id>`                                             | Report every condition affecting task readiness.                     |
| `arcl context <task-id> [--all]`                                     | Return the task with its spec, plan, blockers, handoff and evidence. |
| `arcl check`                                                         | Sync the workspace, then validate database and graph invariants.     |
| `arcl ready`                                                         | List actionable leaf tasks in deterministic order.                   |
| `arcl next`                                                          | Return the first actionable task, if one exists.                     |
| `arcl next --start [--claim [<NAME>]]`                               | Start the first actionable task, claiming it with `--claim`.         |
| `arcl prime`                                                         | Print the agent loop, open work, and the first ready tasks.          |

`check` syncs the workspace first. When the sync fails, `check` fails and
skips the database checks. Workspace content arcl cannot read or accept fails
it with exit `3`:

- a missing or broken `manifest.toml`;
- a stray entry or a bad file name;
- an entry of the wrong type;
- a file that is not UTF-8 or does not parse;
- a field or relationship the merged graph refuses;
- a newer workspace format.

The report then has `valid: false` and the sync error as its one entry in
`errors`. Human output ends with a short error that points at the report.
Under `--json`, the error on stderr has the code `workspace_invalid` and the
full sync error. Other sync failures keep their usual exit code, error code and
message, and print no report. These include a file removed from one side, for
example by a branch switch (exit `3`), and files the database and the workspace
changed differently (exit `4`). Other commands that only read records warn and
still run.

`list` accepts `--kind`, `--status`, `--priority`, `--release`, `--spec`,
`--plan`, `--phase`, `--parent`, `--assignee` and `--all`. Notes have no
status, so `--status` leaves them out. `ready` and `next` accept `--priority`,
`--spec`, `--plan`, `--phase` and `--parent`. `list` leaves out archived
tasks; `--all` includes them.

`context` leaves archived tasks out of the blockers, dependents and earlier
evidence; `--all` includes them. The task itself always shows.

`next --start` selects and starts the task in one write transaction, so two
concurrent calls never start the same task. The second waits up to 5 seconds
for the first to finish, then takes the next ready task; if the database stays
locked longer, it fails with `database is locked` (exit `1`) and can be run
again. `next --start` selects the task `next` returns, and `--claim` names
the assignee as `task start --claim` does. When nothing is ready it
prints `Nothing to start`, prints nothing with `--plain`, returns a null
`data.task` with `--json`, and exits `0`.

`prime` prints the working loop from the Arc Lightning skill, then the
in-progress tasks with their assignee and the parked tasks, which have none
since parking releases the claim, each with the first line of its latest
handoff, then the first ready tasks, then a pointer to
`arcl context <id>`. Each list shows at most 5 tasks and ends with
`and K more` and the command that lists the rest. Titles and handoff lines are
cut to 80 bytes and assignees to 40, so the output stays within 40 lines and
4 KiB. Control characters in titles and handoffs print escaped, as `\x1b`.
An open task with open gates shows their count after its assignee, as
`[2 open gates]`.
`--plain` prints the same text with full IDs. `--json` returns `initialized`
as `true`, `loop`, the loop as Markdown, and `open` and `ready`, each with
`tasks` and `more`. Each task has `id`, `title`, `status`, `priority`,
`assignee` when set, `handoff`, all unescaped, and `open_gates` when it has
any. JSON keeps up to 200 bytes of a title and 400 of a handoff; a longer
field is cut there and ends with `… [truncated]`, so the envelope stays near
10 KiB. `arcl context <id>` shows the full handoff.

Outside an initialized project, or where `.arcl/arcl.db` is missing, `prime`
prints the one-line message other commands fail with there, which says to run
`arcl init`, and exits `0`, so a session-start hook succeeds in a fresh clone.
`--json` returns `initialized` as `false`, `root` and that `message`. A
database `prime` cannot open still fails.

`tree` leaves out promoted and discarded captures and archived tasks in human,
`--plain` and `--json` output; `--all` includes them. A closed capture or an
archived task named as the root still shows, an archived task with its
subtree. The JSON nodes keep their shape either way, and a node with archived
tasks left out below it adds `archived_tasks`, their count with their
subtasks.

`tree` prints specs first, then tasks outside any spec, then captures, notes
and releases, each kind in ID order. `--plain` keeps one record per row, with
top-level records at depth `0`, and `--json` lists the top-level nodes in the
same order. Both print every record and ignore `--ascii`, `--depth` and
`--style`.

Human output draws a box-drawing tree. Each line holds a status mark and the
record's title, with no ID. A phase adds its completed and total task counts,
such as `7/12`, counting subtasks and archived tasks and leaving out cancelled
tasks. A task with unfinished direct blockers ends with `blocked by` and their
short IDs.

Each run of consecutive completed phases folds into one line. A completed phase
with an unfinished task does not fold, so that task still shows. A run of one
keeps its title, as in `✓ Milestone 6: Shared interface system  done · 1 task`.
A longer run names its range when the part of each title before the first colon
is `Milestone <n>` or `Phase <n>`, with the same word and consecutive numbers,
as in `✓ Milestones 1–4  done · 20 tasks`. Otherwise it reads
`✓ 3 phases  done · 9 tasks`.

A footer sums up the captures, notes and releases, such as
`Inbox: 24 open · Notes: 17 · Releases: v0.1.0, v0.2.0`. It leaves out empty
parts, and an empty project prints none.

| Flag              | Human output                                                                                                                      |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `--all`           | Unfolds every phase, shows archived tasks, and lists captures, notes and releases under `Inbox`, `Notes` and `Releases` headings. |
| `--ascii`         | Draws with `\|--`, `` `-- `` and `\|`, as GNU `tree -A` does, and uses each mark's ASCII form.                                    |
| `--depth N`       | Stops after `N` levels of records, counting top-level records as `1`.                                                             |
| `--style <STYLE>` | Lays the tree out as `folded`, the default, `full` or `outline`.                                                                  |

`--style full` draws the same tree with every phase unfolded. Each record line
ends with the record's short ID, aligned right two spaces past the widest
record line, counted in terminal columns, so a wide character such as `日`
counts as two. Every record line pads to that widest line whatever the
terminal's width, so one long title widens the whole view. Headings and the
footer carry no ID and no padding.

Terminals that draw East Asian Ambiguous characters double-width, common in
CJK locales, misalign the marks and box lines. Use `--ascii` there.

`--style outline` prints a `STATUS`, `ID` and `TITLE` header, then one row per
record, with no box characters and no marks:

```text
STATUS       ID    TITLE
open         W6XC  Roadmap
open         FMEE  . Milestone 7: Local web app
in_progress  5BW4  . . T24: Format apps/web and packages/ui with Prettier
```

`STATUS` is the stored status `--plain` prints. Each level below the top, or
below a section heading, adds `. ` before the title. Rows carry no task counts
or blockers. Phases never fold, and a section heading sits in the `TITLE`
column.

Every style keeps the footer, or the section headings with `--all`, and
follows `--all`, `--ascii`, `--depth` and the colour rules below.
Every style prints control characters in titles escaped, as `\n`, `\t` or
`\x1b`, so a title cannot add a line or send the terminal an escape sequence.

`arcl tree <id>` draws that record and its subtree, never folding the root,
with no headings and no footer.

Colour appears only when stdout is a terminal and `NO_COLOR` is unset.
`FORCE_COLOR` or `CLICOLOR_FORCE` turns it on, and `--color` overrides every
check.

See [Inspecting Work](/guides/inspect-work/) for the query workflow and context
contents.

### Roadmap

`arcl roadmap` prints one Markdown page for a README or a docs site: each
spec with its plans and their phases, then the releases.

```markdown
# Roadmap

## Arc Lightning first release

`arcl-s-01M6WRBXQY9HXH7F0VJNRYZYPW` · in progress · 3/5 tasks

### Roadmap

`arcl-pl-01M6WRC4JVC8QDEQ7HVTR5W1K4` · in progress · 3/5 tasks

| Phase | Status | Tasks |
| --- | --- | --- |
| Blocking for automation | done | 2/2 |
| Project hub | planned | 0/1 |
| *Tasks outside phases* | in progress | 1/2 |

## Releases

| Release | Status | Tasks |
| --- | --- | --- |
| v0.3.0 | done | 1/1 |
```

Task counts match `arcl tree`: completed tasks over all tasks at any depth,
with cancelled tasks left out. A spec or plan counts every task below it. A
release counts the tasks it holds and the tasks under the specs and plans it
holds, each task once. `no tasks` stands in for `0/0`.

Tasks directly under a spec get a `*Tasks outside plans*` line below the
spec's status line. Tasks directly under a plan get a `*Tasks outside
phases*` row after its phases. Each counts those tasks and the tasks below
them, so a spec's plans and its line add up to the spec's total, and a plan's
rows add up to the plan's total. The line or row is `done` when every counted
task is completed, `cancelled` when every task is cancelled, `in progress`
once one is in progress or completed, and `planned` before that.

The status is `done` for a completed record and `cancelled` for a cancelled
one. An open record is `in progress` once a task below it is in progress or
completed, and `planned` before that. Completed specs, plans and phases stay
on the page. A cancelled spec lists no plans or tasks, and a cancelled plan
lists no phases or tasks.

Specs, plans and releases print in ID order, and phases by position, then ID.
The page has no timestamp, so the same records give the same bytes. It leaves
out captures, notes and tasks outside any spec. IDs print as code with no
links. Titles escape Markdown: `|`, `` ` ``, `*`, `_`, `[`, `]`, `<`, `>`,
`~`, `&` and `\` anywhere, a leading `#`, `-`, `+` or `=`, and a trailing run
of `#` after a space, so `Plan ##` keeps its `##`.

`--output <PATH>` (`-o`) writes the page to a file instead, through a
temporary file renamed into place, and creates missing parent directories. The
file holds the bytes `arcl roadmap` prints. The rename gives the file a new
inode owned by the user running `arcl`, so other hard links to it keep the old
page. A `PATH` with no symbolic link in it may name any file, such as
`../site/roadmap.md`. When `PATH` goes through a symbolic link, the page goes
to the file the link points to, and the link stays. That file must be under
the project root: a link to a file outside it, existing or not, exits 3 with
`` `<PATH>` links to `<TARGET>`, outside the project root `<ROOT>`; nothing was written ``,
so a link in a cloned repository cannot aim the write at a file such as
`~/.bashrc`. A `PATH` that is a directory, a pipe, a device such as
`/dev/stdout`, or has no file name such as `.`, exits 3 with
`` `<PATH>` is not a file; `--output` needs the path of a file to write ``.
Human output prints ``Wrote the roadmap to `<PATH>`.``, `--plain` prints the
path, and `--json` returns `{path}`.

`--plain` prints the same page as human output. `--json` without `--output`
returns the page's data: `specs`, each with `plans` and `tasks_outside_plans`,
each plan with `phases` and `tasks_outside_phases`, and `releases`. Each
record has `id`, `title`, the stored `status`, `state` (`done`, `cancelled`,
`in_progress` or `planned`) and `progress` with `done` and `total`.
`tasks_outside_plans` and `tasks_outside_phases` have `state` and `progress`,
or are `null` when there are no such tasks. `--quiet` prints nothing, and with
`--output` still writes the file.

## GraphQL queries

| Command                                                              | Purpose                                    |
| -------------------------------------------------------------------- | ------------------------------------------ |
| `arcl query '<document>' [--variables <JSON>] [--operation <NAME>]`  | Run a query given on the command line.     |
| `arcl query --file <PATH> [--variables <JSON>] [--operation <NAME>]` | Run a query from a file, or `-` for stdin. |
| `arcl query --schema`                                                | Print the schema in SDL.                   |

`arcl query` runs a read-only GraphQL query against the project's records
and prints the response as JSON. It needs no server. A response with errors
exits `3`. [GraphQL and REST API](/guides/api/) lists the root fields, the
error codes and the query limits.

<div id="api-only"></div>

## Web app

| Command                      | Purpose                                             |
| ---------------------------- | --------------------------------------------------- |
| `arcl serve [--port <PORT>]` | Serve the web app on `127.0.0.1` until Ctrl-C.      |
| `arcl serve --api`           | Serve only `/api` and `/graphql`, without the page. |

`arcl serve` prints the URL to open, with a per-run token for the first page
load. Under `--api`, the token comes from `ARCL_SERVE_TOKEN` when it is set;
otherwise each run makes one. `--json` prints one startup line with the URL,
port and token. Outside a project it exits `3` before it binds. See
[Web app](/guides/web-app/) for the views,
[GraphQL and REST API](/guides/api/) for the server and
[REST endpoints](/guides/rest/) for the endpoints.

<div id="publishing-and-syncing-the-artifact"></div>

## Artifact commands

| Command                          | Purpose                                                     |
| -------------------------------- | ----------------------------------------------------------- |
| `arcl web export [--full] <DIR>` | Write the artifact's documents as JSON files under `DIR`.   |
| `arcl web apply <FILE>`          | Apply the edits queued on the artifact, from a file or `-`. |

`export` syncs the workspace first and writes nothing if the sync fails.
`--full` lists every document as changed. `apply` runs each queued entry as
the matching `arcl serve` endpoint does. [claude.ai artifact](/guides/artifact/)
describes the documents and the sync steps.

## Lifecycle

Captures move from `captured` to `promoted` or `discarded`. Specs, plans,
phases and releases use `open`, `completed` and `cancelled`. A container
cannot become terminal while a record below it is open unless the command
includes `--allow-open-children`, which closes only that record.

Tasks use `pending`, `in_progress`, `parked`, `completed` and `cancelled`.
Completion and cancellation are terminal.

Repeating the same terminal command is safe, and switching between completed
and cancelled is rejected. See [Lifecycle](/guides/lifecycle/).

## Outputs

Human output is concise and may use ANSI colors. `--plain` emits stable
unstyled records for shell pipelines. `--quiet` prints nothing on success.
`--json` emits a top-level object with a `format_version` and a
command-specific `data` object containing the action and record fields.

| `format_version` | Commands                                                                                             |
| ---------------- | ---------------------------------------------------------------------------------------------------- |
| `2`              | `show <task-id>`, `task show`, `explain`, `task explain`, `context`, `task context`, `task complete` |
| `1`              | Every other command, failures, and sync warnings                                                     |

Task views and context packets report `readiness` with `ready`, `blocked`, and
`reasons`. Each reason has a `kind`: `status`, `has_children`,
`closed_ancestor`, `closed_container`, `direct_blocker`, `inherited_blocker`,
`cancelled_blocker`, `missing_blocker`, `invalid_edge` or `open_gate`. It
also has a `message` and the `task`, `ancestor`, `blocker`, `container`,
`path` or `gate` fields its kind names.
[Automation](/guides/automation/#read-readiness-reasons) lists the fields per
kind.

Under `--json`, failures go to standard error as
`{"format_version": 1, "error": {"code", "exit_code", "message"}}`.

## Status codes

The CLI exits with these codes:

| Code | Meaning                                                                                       |
| ---- | --------------------------------------------------------------------------------------------- |
| `0`  | Success.                                                                                      |
| `1`  | Runtime, filesystem, Git operation, or SQLite failure.                                        |
| `2`  | Command-line parsing failure reported by `clap`.                                              |
| `3`  | Invalid project, record, input, or lifecycle transition.                                      |
| `4`  | Workspace or concurrent-modification conflict, skill drift, or an unconfirmed `setup` change. |
| `5`  | Requested record was not found.                                                               |
