---
title: Manual
description: Complete command and output reference for the arcl CLI.
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
| `--quiet`                     | Suppress explanatory output and print only affected IDs for mutations.           |
| `--help`                      | Print help for the current command.                                              |
| `--version`                   | Print the installed version.                                                     |

## Record IDs

A full record ID is `arcl-<kind>-<ULID>`, such as
`arcl-t-01M5VDDD42HZQ1FEN49X80GBA5`. Every command that takes a record ID also
takes a short ID: the end of the ULID, at least 4 characters, such as `GBA5`.
Put the kind and a dash in front to match only that kind: `t-GBA5`. The kinds
are `c`, `r`, `s`, `pl`, `ph`, `t` and `n`. Letters match in either case.

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

## Project Initialization

### `arcl init`

Initialize or rediscover the project in the enclosing Git worktree.

```sh
arcl init
arcl init --workspace
```

The command creates `.arcl/config.toml`, `.arcl/arcl.db`, and a scoped
`.arcl/.gitignore`. It rejects directories outside Git worktrees and bare
repositories. Re-running initialization preserves existing project state.

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

## Idea commands

| Command                                | Purpose                                    |
| -------------------------------------- | ------------------------------------------ |
| `arcl idea create <title>`             | Capture a new idea.                        |
| `arcl idea update <id>`                | Replace an idea title or description.      |
| `arcl idea discard <id>`               | Soft-delete an idea as `discarded`.        |
| `arcl idea list`                       | List all ideas in the inbox.               |
| `arcl idea promote <id> --spec <PATH>` | Create a linked epic from a captured idea. |

Descriptions use either `--description <MARKDOWN>` or
`--description-file <PATH|->`. The two sources are mutually exclusive; `-`
reads UTF-8 Markdown from standard input.

Promotion accepts `--release <ID>`. It creates the epic and relationship and
marks the idea promoted in one transaction. Repeating a successful promotion
returns the existing epic.

## Release commands

| Command                       | Purpose                   |
| ----------------------------- | ------------------------- |
| `arcl release create <title>` | Create an open release.   |
| `arcl release update <id>`    | Replace release metadata. |
| `arcl release complete <id>`  | Complete a release.       |
| `arcl release cancel <id>`    | Cancel a release.         |

`complete` and `cancel` accept `--allow-open-children` to bypass the default
descendant check for the selected release only.

## Epics

| Command                                  | Purpose                                             |
| ---------------------------------------- | --------------------------------------------------- |
| `arcl epic create <title> --spec <PATH>` | Create an epic linked to an existing Markdown spec. |
| `arcl epic update <id>`                  | Replace epic metadata or its linked spec.           |
| `arcl epic complete <id>`                | Complete an epic.                                   |
| `arcl epic cancel <id>`                  | Cancel an epic.                                     |

Epic creation accepts `--release <ID>`. Epic updates accept either
`--release <ID>` or `--no-release`. Spec paths are validated as regular `.md`
files inside the current Git worktree.

## Milestones

| Command                                     | Purpose                                 |
| ------------------------------------------- | --------------------------------------- |
| `arcl milestone create <title> --epic <ID>` | Create an ordered milestone.            |
| `arcl milestone update <id>`                | Replace milestone metadata or position. |
| `arcl milestone complete <id>`              | Complete a milestone.                   |
| `arcl milestone cancel <id>`                | Cancel a milestone.                     |

Use `--position <N>` when creating or updating a milestone. The default position
is `0`; negative positions are invalid. Terminal commands accept
`--allow-open-children`.

## Task Management

| Command                                     | Purpose                                                 |
| ------------------------------------------- | ------------------------------------------------------- |
| `arcl task create <title> --milestone <ID>` | Create a pending task or subtask.                       |
| `arcl task update <id>`                     | Replace metadata, move a subtree, or change its parent. |
| `arcl task start <id>`                      | Move ready pending work to `in_progress`.               |
| `arcl task park <id>`                       | Move pending or in-progress work to `parked`.           |
| `arcl task unpark <id>`                     | Return parked work to `pending`.                        |
| `arcl task handoff <id>`                    | Store a resume note and park in-progress work.          |
| `arcl task complete <id>`                   | Mark pending or in-progress work completed.             |
| `arcl task cancel <id>`                     | Cancel non-terminal work.                               |

Task creation accepts `--parent <ID>`, `--priority <critical|high|normal|low>`
(default `normal`), and `--position <N>` (default `0`). Task updates accept
`--milestone <ID>`, `--parent <ID>`, or `--no-parent`. `--parent` and
`--no-parent` are mutually exclusive.

Moving a task between milestones moves its entire descendant subtree. Parent and
child rows must belong to the same milestone, and cyclic reparenting is rejected
before storage changes.

`start` refuses a pending task that is not ready and lists the reasons. Having
child tasks is the one reason it skips. `--force` starts the task anyway.

`handoff` requires either `--note <MARKDOWN>` or `--note-file <PATH|->`.

Completion accepts either `--evidence <MARKDOWN>` or `--evidence-file <PATH|->`.
Each pair is mutually exclusive, and `-` reads UTF-8 Markdown from standard input.

`complete` refuses a task with open descendants. `--allow-open-children`
completes only the task. `--with-subtasks` also completes its pending and
in-progress descendants in one write, and refuses, writing nothing, while a
descendant is parked or a blocker outside the subtree is unfinished. The two
flags conflict. JSON output from `task complete` includes `completed_subtasks`,
the IDs it completed in tree order, and `unblocked`, the IDs of tasks that were
not ready before the command and are ready after it. Plain output prints the
task ID, then a `subtask<TAB><id>` line for each completed subtask and an
`unblocked<TAB><id>` line for each task it made ready.

## Inspection

| Command                                                              | Purpose                                                            |
| -------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `arcl show <id>`                                                     | Show one record by its full or short ID.                           |
| `arcl list`                                                          | List records using kind, state, priority, and association filters. |
| `arcl tree [<id>] [--all] [--ascii] [--depth <N>] [--style <STYLE>]` | Show the complete hierarchy or one rooted subtree.                 |
| `arcl explain <task-id>`                                             | Report every condition affecting task readiness.                   |
| `arcl context <task-id>`                                             | Return the bounded context needed to work on a task.               |
| `arcl check`                                                         | Validate database and graph invariants.                            |
| `arcl ready`                                                         | List actionable leaf tasks in deterministic order.                 |
| `arcl next`                                                          | Return the first actionable task, if one exists.                   |

`list` accepts `--kind`, `--status`, `--priority`, `--release`, `--epic`,
`--milestone`, and `--parent`. `ready` and `next` accept the same priority and
association filters.

`tree` leaves out promoted and discarded captures in human, `--plain` and
`--json` output; `--all` includes them. A closed capture named as the root
still shows. The JSON nodes keep their shape either way.

`tree` prints specs first, then tasks outside any spec, then captures, notes
and releases, each kind in ID order. `--plain` keeps one record per row, with
top-level records at depth `0`, and `--json` lists the top-level nodes in the
same order. Both print every record and ignore `--ascii`, `--depth` and
`--style`.

Human output draws a box-drawing tree. Each line holds a status mark and the
record's title, with no ID. A phase adds its completed and total task counts,
such as `7/12`, counting subtasks and leaving out cancelled tasks. A task with
unfinished direct blockers ends with `blocked by` and their short IDs.

Each run of consecutive completed phases folds into one line. A completed
phase with an unfinished task does not fold, so that task still shows. A run of
one keeps its title, as in
`✓ Milestone 6: Shared interface system  done · 1 task`. A longer run names its
range when the part of each title before the first colon is `Milestone <n>` or
`Phase <n>`, with the same word and consecutive numbers, as in
`✓ Milestones 1–4  done · 20 tasks`. Otherwise it reads
`✓ 3 phases  done · 9 tasks`.

A footer sums up the captures, notes and releases, such as
`Inbox: 24 open · Notes: 17 · Releases: v0.1.0, v0.2.0`. It leaves out empty
parts, and an empty project prints none.

| Flag              | Human output                                                                                                                      |
| ----------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `--all`           | Unfolds every phase and lists captures, notes and releases under `Inbox`, `Notes` and `Releases` headings in place of the footer. |
| `--ascii`         | Draws with `\|--`, `` `-- `` and `\|`, as GNU `tree -A` does, and uses each mark's ASCII form.                                    |
| `--depth N`       | Stops after `N` levels of records, counting top-level records as `1`.                                                             |
| `--style <STYLE>` | Lays the tree out as `folded`, the default, `full` or `outline`.                                                                  |

`--style full` draws the same tree with every phase unfolded. Each record line
ends with the record's short ID, aligned right two spaces past the widest
record line, counted in characters. Headings and the footer carry no ID.

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

`arcl tree <id>` draws that record and its subtree, never folding the root,
with no headings and no footer.

Colour appears only when stdout is a terminal and `NO_COLOR` is unset.
`FORCE_COLOR` or `CLICOLOR_FORCE` turns it on, and `--color` overrides every
check.

See [Inspecting Work](/guides/inspect-work/) for the query workflow and context contents.

## Web app

`arcl serve` serves the web app and its endpoints on `127.0.0.1`, on an
ephemeral port unless `--port` pins one. It prints the URL to open, which
carries a per-run token for the first page load. The app moves the token out
of the URL and sends it in the `X-Arcl-Token` header on every later request.
The server refuses a request without the token or with a `Host` header that
does not name the server. Reads take GET and changes take POST; the server
refuses any other method, so a GET never changes records. A change also needs
an `Origin` header naming the server, which stops other pages, including
those on other `127.0.0.1` ports, from sending one. Responses carry no CORS
headers. The page's Content-Security-Policy runs only the app's own script and
loads images only from the server or `data:` URLs, so Markdown cannot pull
images from other hosts. The server runs until Ctrl-C, then gives open
requests up to 5 seconds to finish and exits. A change that has not started
by Ctrl-C gets status 503 and does not run; one already running finishes. A
second Ctrl-C exits at once.

The server closes a connection that takes more than 5 seconds to send a request
head or idles that long between requests. It also closes one whose client reads
replies slower than 64 KiB/s. Each connection starts with 5 seconds of waiting
for its client to read, spends it while a reply waits, earns back a second per
64 KiB the client reads, and never holds more than 5 seconds. A request it
refuses for a bad `Host` or token, for a body over 1 MiB, or for a body that
does not arrive within 5 seconds gets `Connection: close` and the connection
closes after the reply. It keeps at most
128 connections open; a new one waits to be accepted until another closes.

| Endpoint                           | Same JSON as             |
| ---------------------------------- | ------------------------ |
| `GET /api/records`                 | `arcl list`              |
| `GET /api/records/<id>`            | `arcl show <id>`         |
| `GET /api/records/<id>/source`     | none (see below)         |
| `GET /api/ready`                   | `arcl ready`             |
| `GET /api/tasks/<id>/explain`      | `arcl explain <task-id>` |
| `GET /api/tasks/<id>/context`      | `arcl context <task-id>` |
| `GET /api/tree`                    | `arcl tree`              |
| `GET /api/work`                    | none (see below)         |
| `POST /api/tasks`                  | `arcl task create`       |
| `POST /api/tasks/<id>/update`      | `arcl task update`       |
| `POST /api/tasks/<id>/start`       | `arcl task start`        |
| `POST /api/tasks/<id>/park`        | `arcl task park`         |
| `POST /api/tasks/<id>/unpark`      | `arcl task unpark`       |
| `POST /api/tasks/<id>/handoff`     | `arcl task handoff`      |
| `POST /api/tasks/<id>/complete`    | `arcl task complete`     |
| `POST /api/captures`               | `arcl capture create`    |
| `POST /api/captures/<id>/promote`  | `arcl capture promote`   |
| `POST /api/captures/<id>/discard`  | `arcl capture discard`   |
| `POST /api/specs`                  | `arcl spec create`       |
| `POST /api/plans`                  | `arcl plan create`       |
| `POST /api/plans/<id>/apply`       | `arcl plan apply`        |
| `POST /api/records/<id>/edit`      | none (see below)         |

`/api/records` and `/api/ready` take the command's flags as query parameters,
such as `?kind=task&status=pending&status=parked`. `/api/tree` takes none and
returns the tree of every record. `/api/work` takes none and
returns the Work list in one request: every task with the readiness
`arcl explain` reports and its phase ID, inherited from an ancestor where the
task has none, and every phase. Its envelope is
`{"format_version": 2, "data": {"tasks": [{"task", "readiness", "phase_id"}], "phases": [...]}}`.

A POST takes a JSON object of the command's options, named like its flags
with `_` for `-`:

| Endpoint   | Options                                                                                 |
| ---------- | --------------------------------------------------------------------------------------- |
| `tasks`    | `title` (required), `body`, `priority`, `parent`, `spec`, `plan`, `phase`, `blocked_by` |
| `update`   | `title`, `body`, `priority`                                                             |
| `start`    | `force`                                                                                 |
| `handoff`  | `note`                                                                                  |
| `complete` | `evidence`, `allow_open_children`, `with_subtasks`                                      |
| `captures` | `title` (required), `body`                                                              |
| `promote`  | `to` (required), `title`, `body`, `acceptance_criteria`, `spec`, `plan`, `phase`, `parent`, `priority` |
| `specs`    | `title` (required), `body`, `acceptance_criteria`                                       |
| `plans`    | `title` (required), `spec` (required), `body`                                           |
| `apply`    | `document` (required)                                                                   |
| `edit`     | `source` (required), `revision` (required)                                              |

`blocked_by` is a list of task IDs. `to` is `task`, `spec` or `note`, and
`promote` uses the capture's title and body for any it leaves out. `document`
is the plan's TOML, as `arcl plan apply --file` reads it.

`/api/records/<id>/source` returns the record's workspace file, the bytes
`arcl workspace export` writes for it, as
`{"format_version": 1, "data": {"id", "kind", "path", "source", "revision"}}`.
`path` is the file's place in the workspace, such as `tasks/<id>.md`, and
`revision` is the lowercase hex SHA-256 of `source`. `edit` replaces the
record with `source`, a whole file, and answers with the saved file in the
same shape. It checks the file as `arcl workspace import` checks one: the TOML
must parse, the fields must fit the record's kind, the ID must match the
record, and every relationship and blocker rule must hold for the whole
graph. The edit is compared with the database, as the task commands are, so
a blocker cycle or ancestry rule violation it adds is refused even before the
first sync; one already in the database loads with a warning.

An edit may not change a field that only a command changes, since those
commands check lifecycle rules, readiness and open subtasks that the import
does not. Such an edit fails with `invalid_request`, status 400, and the
message names the command. A field left at its value passes, so a closed
record's other fields stay editable.

| Kind    | Fields only a command changes           | Commands                                                 |
| ------- | --------------------------------------- | -------------------------------------------------------- |
| task    | `status`, `handoff`, `evidence`         | `arcl task start`, `park`, `unpark`, `handoff`, `complete`, `cancel` |
| capture | `status`, `promoted-to`                 | `arcl capture promote`, `discard`                        |
| spec    | `status`, `source-capture`              | `arcl spec complete`, `cancel`; `arcl capture promote`   |
| plan    | `status`                                | `arcl plan complete`, `cancel`                           |
| phase   | `status`                                | `arcl plan phase complete`, `cancel`                     |
| release | `status`                                | `arcl release complete`, `cancel`                        |

The record ID in the path is the only file the request names. When the
file's revision is no longer `revision`, such as after an edit in Git or
another change, the edit fails with `conflict`, status 409. A refused edit
changes nothing. A saved edit reaches the workspace file through the sync
after the change.

`park`, `unpark` and `discard` take an empty body or `{}`. An unknown option, a body that
is not JSON, or a body over 1 MiB is refused. A request whose
`Content-Length` is over 1 MiB gets status 413 before any other check. The
server reads a body only after the request passes its checks, and answers 408
and closes the connection when the body has not arrived after 5 seconds. A
failure returns the CLI's error envelope with status 400 for
exit codes 2 and 3, 404 for 5, 409 for 4, and 500 otherwise.

A read opens the database without syncing the workspace, takes no lock and
never changes records; opening the database can still run its migrations. A
change runs as the CLI runs it: under the project's sync lock, after a sync
that must succeed, followed by a sync that exports it. Where a sync or export
warns, the reply's envelope lists the warnings in `warnings`, as the CLI's
JSON output does. A change waits at most 3 seconds for the lock; if another
`arcl` command still holds it, the change fails with `application_error`,
status 503 and a `Retry-After` header, and records stay as they were. Other
`arcl` commands run while the server serves. Outside a project, `arcl serve`
exits with code 3 before it binds.

The Work view lists every task by milestone, then by status, in the order
`arcl tree` gives: milestones by plan and position, and tasks by position,
each followed by its subtasks. A glyph leads each row; for a pending or parked
task, hover over it or focus the row to read why the task is not ready. `j` and `k` move the selection, and `Enter`
opens the task. Filters narrow the list by status, priority, milestone and
readiness: ready, blocked by an incomplete task, or waiting for another
reason, such as open subtasks. The page URL keeps the filters. A task deleted
while the list loads is left out; any other failed read shows an error with a
Retry button. **New task** opens a form for the title, priority and
description, creates the task with `arcl task create`, and opens it.

**List** and **Board** beside the title, or `v`, switch the layout. The
choice is remembered in this browser, and the URL names the board as
`?view=board`. The board has a column for each status: Pending, In progress,
Parked, Completed and Cancelled. Its cards show the task's ID, title,
readiness glyph and priority, link to the Task view, and follow the filters.
A long column scrolls inside itself, and a narrow window scrolls the board
sideways. Dragging a card to another column runs the command that makes that
move:

| From                 | To          | Runs                 |
| -------------------- | ----------- | -------------------- |
| Pending              | In progress | `arcl task start`    |
| Pending, In progress | Parked      | `arcl task park`     |
| Parked               | Pending     | `arcl task unpark`   |
| Pending, In progress | Completed   | `arcl task complete` |

A move to Completed first asks for optional evidence. The web app has no
cancel action, so a drop on Cancelled is refused with a message that names
`arcl task cancel`. No command reopens a closed task. Any other drop the
table leaves out is refused with a message too, and the card stays in its
column. While a move runs,
no other card moves. A card moves once its command succeeds; a
refused change shows the CLI's message. Each card's **Move** button, or `m`
on the selected card, opens a dialog with the moves it allows, so the
keyboard reaches every move.

The Task view shows what `arcl show <task-id>` returns: why the task is not
ready, its direct blockers, the blockers it inherits from each ancestor, its
subtasks, the tasks it unblocks, the latest handoff and the evidence. Each
task it names links to that task's view. The description renders as Markdown,
and **View Markdown** opens the source in a read-only editor. The properties
panel lists status, readiness, priority, parent, spec, plan and phase; `]`
collapses and expands it, also from inside the read-only source. `Escape`
returns to the Work list with its filters and the task selected, except while
an open form holds unsaved input; inside the source, the first `Escape` moves
focus out of the editor. A failed read shows an error with a Retry button.

Buttons under the title change the task. A pending task offers Start and
Park, an in-progress task Complete, Hand off and Park, and a parked task
Unpark. Complete asks for optional Markdown evidence and Hand off for a
Markdown note, which parks the task. **Edit** opens the title, priority and
description in a writable editor; Save sends only the fields that changed.
Each change runs the `arcl task` command of the same name, so the CLI's
validation applies. A refused change shows the CLI's message and error code
next to its button. A saved change whose sync or export warned shows the
warnings in a notice. After a change the view reloads the task, and after a
completion it lists the tasks the completion made ready.

**Edit file** opens the task's whole workspace file in an editor, with the
TOML front matter between the `+++` lines highlighted as TOML and the body as
Markdown. **Save file** sends it to `POST /api/records/<id>/edit`, so the
file passes the checks `arcl workspace import` runs. The file cannot change
the task's status, handoff note or evidence; the buttons and the board do
that. A refused file shows the CLI's message and keeps the edit open; when
the file changed since it was opened, **Reload the file** loads the new
version in place of the edit.

The Inbox lists the captures not yet promoted or discarded, oldest first, and
shows the selected one's body. `j` and `k` move the selection. **Promote to
task**, **Promote to spec** and **Make a note** open a form with the
capture's title, which runs `arcl capture promote`; the new record gets the
capture's body. The task form sets the priority, milestone and parent, and
reads `!priority` and `^REF` from the capture's title. A promotion takes no
blockers, so the form names each `after:REF` for you to add with
`arcl dependency add`. The spec form takes optional acceptance criteria.
**Discard** asks first, then runs `arcl capture discard`. After either, the
next capture is selected, and a notice links to the new task or to Planning.
While a form holds edits, or a promotion or discard is running, `j`, `k` and
clicking another capture keep the selection; with edits, they return focus to
the form. A capture already promoted or discarded elsewhere, such as from the
CLI, reloads the list with a notice.

Planning lists each spec with its plans, and each plan with its milestones in
`arcl tree` order. A milestone row shows whether the phase is open or done,
how many of its tasks are done, and how many are ready or blocked; subtasks
count. `j` and `k` move between specs, plans and milestones, and `Enter`
opens the selected milestone's tasks and subtasks below it, or collapses and
expands a spec or plan. Closed specs and plans start collapsed. A task in the
tree opens its Task view. **New spec** runs `arcl spec create`, **New plan**
on a spec runs `arcl plan create`, and **Apply plan** on a plan takes a TOML
document and runs `arcl plan apply`. Applying again with the same keys updates
the plan's phases and tasks. The view reloads in place after each change.

`⌘K`, or `Ctrl+K` off macOS, opens the command menu; so does **Search** in the
sidebar. The menu lists the actions on the selection first: the row
selected in the Work list, the Task view's task, the Inbox's capture, or the
selected spec or plan in Planning. The open view's
commands, Quick capture and Go to follow, and once you type, the tasks whose
titles match. Each command shows its shortcut. Typing filters the list, the
arrow keys move, `Enter` runs the command and `Escape` closes the menu.
`Backspace` in an empty search drops the selected task from the menu. A
command runs the same change as its button; from the Work list, Complete and
Hand off open the Task view with their form.

| Keys          | Command                                       |
| ------------- | --------------------------------------------- |
| `⌘K` `Ctrl+K` | Open the command menu                         |
| `q`           | Quick capture                                 |
| `g` `i`       | Go to Inbox                                   |
| `g` `w`       | Go to Work                                    |
| `g` `p`       | Go to Planning                                |
| `n`           | New task, in the Work list                    |
| `s`           | Start the selected task                       |
| `c`           | Complete the selected task                    |
| `h`           | Hand off the selected task                    |
| `p`           | Park the selected task                        |
| `u`           | Unpark the selected task                      |
| `e`           | Edit the task, in the Task view               |
| `v`           | Switch between list and board, in Work        |
| `m`           | Move the selected card, on the board          |
| `j` `k`       | Select the next or previous task, in Work     |
| `Enter`       | Open the selected task, in Work               |
| `]`           | Collapse or expand properties, in a task      |
| `Escape`      | Return to Work, in a task                     |
| `j` `k`       | Select the next or previous row, in Inbox and Planning |
| `1`           | Promote the capture to a task, in Inbox       |
| `2`           | Promote the capture to a spec, in Inbox       |
| `3`           | Make the capture a note, in Inbox             |
| `Backspace`   | Discard the capture, in Inbox                 |
| `n`           | New spec, in Planning                         |
| `a`           | New plan on a spec, or Apply plan on a plan, in Planning |
| `Enter`       | Open or close the selected row, in Planning   |
| `Escape`      | Close the open form if it has no edits, in Inbox and Planning |

Shortcuts do nothing while you type in a field, the editor or a dialog, or
hold `Ctrl`, `⌘` or `Alt`. Press `g` and its second key within a second.

`q`, or **Capture** in the sidebar, opens quick capture from any view. The
first line is the title, and `Shift+Enter` starts the body. `Enter` files the
text with `arcl capture create`. `⌘Enter`, or `Ctrl+Enter` off macOS, files it
as a task with `arcl task create` and opens it; tokens in the first line then
set the task's fields:

| Token       | Sets                                                       |
| ----------- | ---------------------------------------------------------- |
| `!high`     | The priority, from `!critical`, `!high`, `!normal`, `!low` |
| `^REF`      | The parent task                                            |
| `after:REF` | A blocker; repeat it for more than one                     |

`REF` is a task ID, such as `arcl-t-01M5…`, or a task number, such as `T18.4`
for `T18.4: Add task mutations`. A reference that names no task or several
stops the task with an error. With two `^REF`, only the last is looked up.
Only a whole word is a token, so `!later`, `^C` and `after:lunch` stay text,
and the body is never read for tokens. A leading backslash keeps a token as
text: `\!high` puts `!high` in the title. A capture holds only a title and a
body, so `Enter` keeps the first line as typed, tokens and backslashes
included.

A change from the menu or quick capture whose sync warned shows the warnings in
a notice, as the Task view does.

The theme follows the system's light or dark setting: light grey and blue, or
black and blue. Text in both themes meets WCAG AA contrast. Animations and
transitions stop when the system asks for reduced motion. In a window narrower
than 48rem the sidebar becomes a bar at the top, and **Menu** shows Search,
Capture and the views; choosing a view closes it.

`arcl` embeds the web app from `apps/web/build` when it compiles. That build is
not committed, so `cargo install --git` produces an `arcl` whose `serve` shows
a page saying the web app was not built. To get the web app, run
`pnpm install` and `pnpm --filter @arclightning/web build` in a checkout, then
`cargo install --path .`.

## Publishing and syncing the artifact

The web app also runs as a claude.ai artifact, so a person can browse and
triage a project from claude.ai without `arcl serve`. The artifact cannot
reach the repository. An agent session copies the project's records into the
artifact's database and applies the edits people queue on the page.

`pnpm --filter @arclightning/web build:artifact` writes the artifact page to
`apps/web/build-artifact/index.html`: the same views as `arcl serve`'s page,
in one file with no external scripts. It routes on the URL's hash, such as
`#/tasks/<id>`, sends no token and never calls `/api`. Publish it with this
capability declaration:

```json
{
  "db": {
    "rules": [
      { "path": "", "read": "view", "write": "admin" },
      { "path": "queue", "read": "view", "write": "interact" }
    ]
  },
  "user": {}
}
```

Everyone the artifact admits reads the records. Contributors and above queue
edits; only editors and the owner write records. Opened outside claude.ai, or
without the database, the page says to open it in claude.ai.

`arcl web export <dir>` syncs the workspace, then writes the database's
documents under `<dir>` with the same reads `arcl serve` runs. If the sync
fails, it exits non-zero and writes nothing, so `meta/sync` never stamps
records the sync did not update:

| Document         | File                  | Same JSON as                                        |
| ---------------- | --------------------- | --------------------------------------------------- |
| `views/work`     | `views/work.json`     | `GET /api/work`                                     |
| `views/tree`     | `views/tree.json`     | `GET /api/tree`                                     |
| `views/tasks`    | `views/tasks.json`    | `GET /api/records?kind=task`                        |
| `views/captures` | `views/captures.json` | `GET /api/records?kind=capture&status=captured`     |
| `records/<id>`   | `records/<id>.json`   | `GET /api/records/<id>`, per task and open capture  |
| `sources/<id>`   | `sources/<id>.json`   | `GET /api/records/<id>/source`, per task            |
| `meta/sync`      | `meta/sync.json`      | the commit, time and revision of the export         |

`<dir>/manifest.json` lists every document in `documents`, the ones that are
new or differ from the files of the last export into `<dir>` in `changed`,
and the ones the last export had and this one does not in `removed`. A sync
writes the `changed` documents in order and deletes the `removed` ones, so it
touches only the records that changed. `changed` compares against `<dir>`,
not the database, so after an upload that did not finish, or into a
database `<dir>` never described, run `arcl web export --full <dir>`. It
lists every document in `changed`.

A document over 200 KiB is written as parts, `chunks/<collection>~<id>~<n>`,
each `{"part": "..."}`, listed before a header
`{"chunked": {"parts": n, "digest": "..."}}` at the document's own path. The
page joins the parts and checks them against `digest`, the 64-bit FNV-1a hash
of the joined text; parts that do not match are read as a sync in progress. A document needs at most 64 parts, about 12.5 MiB.
`export` fails on a larger one.

A change on the page writes a `queue/<id>` document instead of changing a
record:

```json
{
  "op": "discard_capture",
  "target": "arcl-c-...",
  "input": {},
  "created_at": "2026-10-25T06:30:00.000Z",
  "created_by": "u_...",
  "status": "queued"
}
```

`op` names the change: `create_task`, `update_task`, `start_task`,
`park_task`, `unpark_task`, `hand_off_task`, `complete_task`,
`create_capture`, `promote_capture`, `discard_capture`, `create_spec`,
`create_plan`, `apply_plan` or `update_source`. `target` is the record ID for
those that name one and `null` otherwise. `input` holds the options of the
matching `arcl serve` POST endpoint. The page says the edit is queued, keeps
showing the records as they are, and lists the queued edits above each view.
A board move queues the same way, and its card stays put until the edit
applies. The page refuses an edit whose entry is over 200 KiB as UTF-8 JSON
and says to make it smaller or make it with `arcl`.

**Edit file** reads the task's file from `sources/<id>` and **Save file**
queues `update_source` with `{"source", "revision"}`: the whole file and the
revision from that document. `apply` runs it as
`POST /api/records/<id>/edit`, with the same checks. If the file changed
after the export the page read, the entry fails with the `conflict` error;
open the file again after the next sync and redo the edit. Any change to the
task that applies first rewrites its file, whether a board move, an edit or
another raw edit, so the raw edit then fails the same way. Saving closes the
editor, and while any edit to the task waits for the sync, queued or
applying, the page disables **Edit file** and says to edit the file after
that sync.

`arcl web apply <file>` reads a JSON array of queue entries, each the
document's fields plus its `id`, from a file or `-` for standard input. It
runs each entry whose `status` is `queued` through the same code as
`arcl serve`'s endpoint: the same validation, sync lock and export, waiting up
to 60 seconds for the lock. An entry with any other status, or none, is
skipped; the page does not list it either. A file that is not a JSON array
exits with code 3 and applies nothing. An entry that is not a valid queue
entry, such as one with a field the page does not write, fails with an
`invalid_request` error naming its position and `id`, and the other entries
still run. So does an entry whose `input` is over 1 MiB as JSON, the
endpoints' body limit. Under `--json` it prints
`{"format_version": 1, "data": {"results": [...]}}`, one result per entry in
order: `id`, `op`, `target` (each `null` when a malformed entry has no string
value for it), `status` (`applied`, `failed` or `skipped`), the command's
`data` when it applied, the CLI's `error` when it failed, and `warnings`.

Anyone who can queue an edit can also change or delete another queued entry,
so the queue is untrusted input. `apply` checks every entry as the CLI checks
a command, and an applied edit runs as the person who runs `arcl`.

To sync, the agent session that owns the artifact:

1. Reads the `queue` collection. An entry whose `status` is `applying` was
   claimed by a sync that stopped before it finished. Check whether its
   change is in the records: if it is, delete the entry in step 5 with this
   sync's applied ones; if it is not, set `status` back to `queued`.
2. Sets `status` to `applying` on each `queued` entry in the database, then
   writes those entries, with their IDs and `status` still `queued`, to a
   file. An entry claimed this way never runs twice: `apply` skips it if a
   later sync reads it again.
3. Runs `arcl --json web apply <file>`, then sets `status` to `failed` and
   `error` to the result's error on each failed entry, so the page shows
   why. Applied entries stay `applying` for now.
4. Runs `arcl --json web export <dir>` into the directory of the last sync,
   writes each `changed` document from `<dir>/<path>.json` and deletes each
   `removed` one. If the export fails because the workspace did not sync,
   resolve that, with `arcl workspace status`, and run it again. If a write
   fails, or `<dir>` is new, the next sync runs
   `arcl --json web export --full <dir>`, writes every `changed` document
   and deletes every document in `views`, `records`, `sources` and `chunks`
   that `documents` does not list.
5. Deletes each applied entry. This comes after the writes, so the page never
   offers **Edit file** on a `sources/<id>` document older than the edit. If
   a write in step 4 failed, stop before this step: the applied entries stay
   `applying`, and step 1 of the next sync finds their changes in the
   records.

The page follows `meta/sync` and the queue. When `revision` changes, it offers
to reload the view with the new records. It lists `applying` entries with the
queued ones, marked as applying now. Dismissing a failed edit on the page
deletes its entry. If the database ends the page's subscription for good, the
page says it stopped following syncs and to reload.

## Lifecycle

Container records—releases, epics, and milestones—use `open`, `completed`, and
`cancelled`. A container cannot become terminal while a descendant is
non-terminal unless the command includes `--allow-open-children`.

Task records use `pending`, `in_progress`, `parked`, `completed`, and
`cancelled`. Completion and cancellation are terminal.

Repeating the same terminal command is safe, and switching between completed
and cancelled is rejected.

## Outputs

Human output is concise and may use ANSI colors. `--plain` emits stable unstyled
records for shell pipelines. `--quiet` prints only the affected ID for a
mutation. `--json` emits a top-level object with a `format_version` and a
command-specific `data` object containing the action and record fields.

| `format_version` | Commands                                                                                             |
| ---------------- | ---------------------------------------------------------------------------------------------------- |
| `2`              | `show <task-id>`, `task show`, `explain`, `task explain`, `context`, `task context`, `task complete` |
| `1`              | Every other command, failures, and sync warnings                                                     |

Task views and context packets report `readiness` with `ready`, `blocked`, and
`reasons`. Each reason has a `kind`: `status`, `has_children`,
`closed_ancestor`, `closed_container`, `direct_blocker`, `inherited_blocker`,
`cancelled_blocker`, `missing_blocker`, or `invalid_edge`. It also has a
`message` and the `task`, `ancestor`, `blocker`, `container`, or `path` fields
its kind names. [Automation](/guides/automation/) lists the fields per kind.

For example:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The JSON mode writes structured failures to standard error and is intended
for automation that needs stable field names rather than human prose.

## Status Codes

The CLI uses these status categories/codes:

| Code | Meaning                                                        |
| ---- | -------------------------------------------------------------- |
| `0`  | Success.                                                       |
| `1`  | Runtime, filesystem, Git operation, or SQLite failure.         |
| `2`  | Command-line parsing failure reported by `clap`.               |
| `3`  | Invalid project, record, input, or lifecycle transition.       |
| `4`  | Workspace or concurrent-modification conflict, or skill drift. |
| `5`  | Requested record was not found.                                |
