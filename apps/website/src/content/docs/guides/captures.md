---
title: Captures
description: File unplanned work in the inbox, then promote each capture to a spec, a task or a note, or discard it.
---

A capture is an inbox entry for work that is not planned yet. It has a title,
a Markdown body and a status: `captured`, `promoted` or `discarded`. Capture
first and sort later, so a thought does not interrupt the task in front of
you.

## Capture

```sh
arcl capture create "Add keyboard shortcuts" --body "Support the common navigation actions."
```

```text
Created capture `arcl-c-01M6Z4W77CXC4VZAJK35G1Y9K7`: Add keyboard shortcuts
```

`--body` takes inline Markdown. `--body-file <PATH>` reads a UTF-8 file, and
`--body-file -` reads standard input:

```sh
printf 'Steps to restore.\n' | arcl capture create "Document backups" --body-file -
```

The web app's quick capture, `q`, and the MCP `capture_create` tool file
captures the same way.

## Record where it came from

A capture found while working on a task can name that task as its source:

```sh
arcl capture create "Found a flaky test" --from 7TR6
```

Without `--from`, the capture takes the one in-progress task claimed by the
name in `ARCL_AGENT`, if there is exactly one. With `ARCL_AGENT=codex` and a
task claimed by `codex` in progress, the capture links to that task:

```text
Created capture `arcl-c-01M6Z53K5FY604VXHYP3Q7KR50`: Found a flaky test
From: task `7TR6`
```

With several such tasks the capture gets no source and the command warns,
naming them. The record a capture is promoted to keeps the source: `show` on
the new spec, task or note reads it through the capture.

## Review the inbox

```sh
arcl capture list
arcl capture show 1Y9K7
arcl capture update 1Y9K7 --title "Add keyboard navigation"
```

```text
capture	arcl-c-01M6Z4W77CXC4VZAJK35G1Y9K7	[captured]	Add keyboard shortcuts
capture	arcl-c-01M6Z4W77W18J8Z09XWF7VN1KS	[captured]	Document recovery
```

`capture list` lists every capture in creation order, closed ones included.
`arcl tree` counts the open ones in its `Inbox` footer, and `arcl tree --all`
lists them. A discarded capture cannot be updated.

## Promote a capture

Promotion turns a capture into a spec, a task or a note. The new record takes
the capture's title and body unless you pass `--title`, `--body` or
`--body-file`. The capture becomes `promoted` and links to the new record.

```sh
arcl capture promote 1Y9K7 spec --acceptance-criteria "- [ ] j and k move the selection"
arcl capture promote EBQ2 task --plan Z234 --priority low
arcl capture promote 4HDA note
```

```text
Promoted capture `arcl-c-01M6Z4W77CXC4VZAJK35G1Y9K7` to spec `arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC`
Promoted capture `arcl-c-01M6Z4WV16AG5CVWFCAPA4EBQ2` to task `arcl-t-01M6Z53K0V8R20KV4R3DX5X14B`
```

The target is a positional argument or `--to <KIND>`. Each target takes its
own options:

| Target | Options                                                                      |
| ------ | ---------------------------------------------------------------------------- |
| `spec` | `--acceptance-criteria <MARKDOWN>` or `--acceptance-criteria-file <PATH\|->` |
| `task` | `--spec`, `--plan`, `--phase`, `--parent`, `--priority`, `--position`        |
| `note` | None beyond the title and body                                               |

Promotion is idempotent: promoting the same capture to the same kind again
returns the record it already made. Promoting it to another kind fails with
exit code `3`:

```text
error: invalid project or record: capture `arcl-c-01M6Z4WV16AG5CVWFCAPA4EBQ2` is already promoted to task; cannot promote it to note
```

A promoted task takes no blockers. Add them afterwards with
`arcl dependency add`.

## Discard a capture

```sh
arcl capture discard 7VN1KS
```

Discarding keeps the record, with status `discarded`, so its history stays in
the workspace. Discarding twice does nothing more. A discarded capture cannot
be updated or promoted. Records are never deleted.
