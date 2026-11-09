---
title: Capture and triage work
description: File unplanned work in the inbox, record the task it came from, then promote each capture to a spec, task or note, or discard it.
---

<div id="capture"></div>
<div id="capture-an-idea"></div>

## File a capture

Give the capture a title and, if it helps, a Markdown body:

```sh
arcl capture create "Add keyboard shortcuts" --body "Support the common navigation actions."
```

```text
Created capture `arcl-c-01M6Z4W77CXC4VZAJK35G1Y9K7`: Add keyboard shortcuts
```

`--body-file <PATH>` reads the body from a UTF-8 file, and `--body-file -`
from standard input:

```sh
printf 'Steps to restore.\n' | arcl capture create "Document backups" --body-file -
```

The web app's quick capture, `q`, and the MCP `capture_create` tool file
captures the same way.

<div id="record-where-it-came-from"></div>

## Record the task it came from

Name the task you were working on with `--from`:

```sh
arcl capture create "Found a flaky test" --from 7TR6
```

```text
Created capture `arcl-c-01M6Z53K5FY604VXHYP3Q7KR50`: Found a flaky test
From: task `7TR6`
```

Without `--from`, the CLI uses the one in-progress task claimed by the name in
`ARCL_AGENT`. With several such tasks the capture gets no source, and the
command warns and names them. The MCP `capture_create` tool records a source
only when the call passes `from`.

The record a capture becomes keeps the source. `show` on the new spec, task or
note reads it through the capture.

<div id="review-and-update-the-inbox"></div>

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

`capture list` shows every capture in creation order, promoted and discarded
ones included. `arcl tree` counts the open ones in its `Inbox` footer, and
`arcl tree --all` lists them.

<div id="promote-ideas"></div>

## Promote a capture

Promote a capture to a spec, a task or a note. Name the target as an argument
or with `--to <KIND>`:

```sh
arcl capture promote 1Y9K7 spec --acceptance-criteria "- [ ] j and k move the selection"
arcl capture promote EBQ2 task --plan Z234 --priority low
arcl capture promote 4HDA note
```

```text
Promoted capture `arcl-c-01M6Z4W77CXC4VZAJK35G1Y9K7` to spec `arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC`
Promoted capture `arcl-c-01M6Z4WV16AG5CVWFCAPA4EBQ2` to task `arcl-t-01M6Z53K0V8R20KV4R3DX5X14B`
```

The new record takes the capture's title and body unless you pass `--title`,
`--body` or `--body-file`. Each target takes its own options:

| Target | Options                                                                      |
| ------ | ---------------------------------------------------------------------------- |
| `spec` | `--acceptance-criteria <MARKDOWN>` or `--acceptance-criteria-file <PATH\|->` |
| `task` | `--spec`, `--plan`, `--phase`, `--parent`, `--priority`, `--position`        |
| `note` | None beyond the title and body                                               |

A promoted task takes no blockers. Add them afterwards with
`arcl dependency add`.

Promoting the same capture to the same kind again returns the record it
already made. Promoting it to another kind fails with exit code `3`:

```text
error: invalid project or record: capture `arcl-c-01M6Z4WV16AG5CVWFCAPA4EBQ2` is already promoted to task; cannot promote it to note
```

<div id="discard-an-idea"></div>

## Discard a capture

```sh
arcl capture discard 7VN1KS
```

The capture stays in the database and the workspace with status `discarded`.
Discarding it again changes nothing. A discarded capture cannot be updated or
promoted.
