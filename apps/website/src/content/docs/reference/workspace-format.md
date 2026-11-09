---
title: Workspace file format
description: The layout of .arcl/workspace, the manifest and its format versions, and the front matter fields of each record file.
---

## Layout

```text
.arcl/workspace/
├── manifest.toml
├── captures/<capture-id>.md
├── specs/<spec-id>.md
├── plans/<plan-id>.md
├── phases/<phase-id>.md
├── tasks/<task-id>.md
├── notes/<note-id>.md
└── releases/<release-id>.md
```

`.arcl/config.toml` turns the workspace on and sets its path:

```toml
format-version = 1

[workspace]
enabled = true
path = ".arcl/workspace"
```

A stray entry or a badly named file under the workspace fails sync and
`arcl check` with exit code `3`.

## Record files

Each record file holds TOML front matter between `+++` lines, then the
record's Markdown body:

```text
+++
id = "arcl-s-01M70M9RCKZAZHY9Q2XNB8KVEY"
title = "Export notes as Markdown"
status = "open"
acceptance-criteria = """
- [ ] `notes export --format md` writes one Markdown file per note
- [ ] Each file keeps the note's title as its first heading"""
+++

Let people take their notes out of the app.
```

An unknown field fails the import. Every kind takes `id`, `title` and
`links`, an array of `{kind, id}` tables that `arcl note link` writes on
notes.

| Kind    | Other fields                                                        |
| ------- | ------------------------------------------------------------------- |
| capture | `status`, `created-at`, `promoted-to` (`{kind, id}`), `source-task` |
| spec    | `status`, `source-capture`, `acceptance-criteria`                   |
| plan    | `spec`, `status`                                                    |
| phase   | `plan`, `plan-key`, `status`, `position`                            |
| task    | see [Task fields](#task-fields)                                     |
| note    | none                                                                |
| release | `status`, `members`, an array of `{kind, id}` tables                |

Front matter stores explicit relationships only. A release member or a note
link never adds the record's descendants.

## Task fields

```toml
id = "arcl-t-01M70MA00ZRHB519KSK58YPVEA"
spec = "arcl-s-01M70M9RCKZAZHY9Q2XNB8KVEY"
plan = "arcl-pl-01M70M9W0RSTBPAZ3S2GXHQRTP"
phase = "arcl-ph-01M70MA00ZHT8234VNQ1A43WW1"
plan-key = "core/render"
title = "Render a note as Markdown"
status = "completed"
priority = "high"
position = 10
blocked-by = ["arcl-t-01M70MA00ZEG3W6128TAW0K5KG"]
handoff = "Headings and lists render."
evidence = "render.rs keeps code block languages."
completed-at = "2026-11-09T17:05:03.346Z"
```

A task also takes `parent`, `assignee` and `archived-at`. Empty optional
fields are left out.

`completed-at` records when an arcl command completed the task, and
`archived-at` when `arcl compact` archived it. Both are RFC 3339 timestamps,
stored in UTC to the millisecond, and only a completed task may carry them. A
task file written before format version 6 has neither, and imports as a
completed task with no completion time.

Only arcl commands set `completed-at`. A file set to `completed` by hand
imports with no completion time. To reopen a task by hand, set its `status`
and delete `completed-at`, and `archived-at` if it has one; sync refuses an
open task that carries either.

### Task notes

Notes follow the other fields, oldest first:

```toml
[[notes]]
created-at = "2026-11-09T17:04:53.704Z"
kind = "decision"
text = "Render with pulldown-cmark's writer, not string templates"
```

`kind` is optional. Import checks each note's text as `arcl task note` does,
and stores `created-at` in UTC to the millisecond. arcl commands only append
notes; editing or deleting one in the file and syncing changes or removes it.

### Gates

Gates follow the notes, oldest first. `met-at` is present exactly when the
gate is met:

```toml
[[gates]]
id = "arcl-g-01M5Z5P10G85D2836X9EYGQYF6"
kind = "pr"
target = "https://github.com/o/r/pull/7"
status = "met"
created-at = "2026-10-27T10:00:00.000Z"
met-at = "2026-10-28T09:00:00.000Z"
```

Import checks each target as `arcl gate add` does.

## Fields only commands change

The web app's file editor and `POST /api/records/<id>/edit` refuse a change to
these fields, since their commands check lifecycle rules the import does not.
A hand edit followed by `arcl workspace sync` is not checked this way.

| Kind                 | Fields                                                                                       |
| -------------------- | -------------------------------------------------------------------------------------------- |
| task                 | `status`, `handoff`, `evidence`, `assignee`, `notes`, `gates`, `completed-at`, `archived-at` |
| capture              | `status`, `promoted-to`, `source-task`                                                       |
| spec                 | `status`, `source-capture`                                                                   |
| plan, phase, release | `status`                                                                                     |

## Manifest and format versions

`manifest.toml` holds one field, `format-version`, from 1 to 6. A sync raises
the version only when a file needs a newer field, and otherwise keeps the
version the manifest has:

| Version | Written once a file carries            |
| ------- | -------------------------------------- |
| `1`     | none of the fields below               |
| `2`     | a task `assignee`                      |
| `3`     | a task note                            |
| `4`     | a task gate                            |
| `5`     | a capture `source-task`                |
| `6`     | a task `completed-at` or `archived-at` |

A new workspace starts at version 1. Releasing every claim leaves a version 2
workspace at 2.

An arcl that supports only older versions refuses a newer workspace before it
reads or writes any file:

```text
workspace format version 7 was written by a newer arcl; this arcl supports workspace format versions up to 6. Upgrade arcl to use this workspace
```

arcl 0.3.0 reads only version 1, and refuses later ones with
`unsupported configuration format version 6; expected 1`, which does not say
to upgrade. The first `task complete` writes version 6, so upgrade every clone
and CI job past 0.3.0 before anyone completes a task.

## Canonical form

After an import, arcl rewrites the workspace in its canonical form. It sorts
files by path and repeated references by kind and ID, uses LF line endings and
one final newline, and leaves out empty optional fields. Export leaves a file
alone when its bytes already match.
