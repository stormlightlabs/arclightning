---
title: Script arcl with JSON output
description: Read arcl from scripts and agents with versioned JSON envelopes, typed readiness reasons, plain or quiet output, and exit codes.
---

Human output is for people and can change between releases. A script picks
`--json`, `--plain` or `--quiet`, which keep a stable shape. An agent that
speaks MCP can [connect the MCP server](/how-to/connect-mcp/) instead.

<div id="give-an-agent-the-skill"></div>
<div id="prime-each-session"></div>
<div id="connect-the-mcp-server"></div>

[Set up an agent](/how-to/set-up-an-agent/) covers the skill and priming each
session.

## Read JSON

Pass `--json` before or after the command:

```sh
arcl --json capture create "Automate this" --body "Details"
```

```json
{"format_version":1,"data":{"action":"created","capture":{"body":"Details","created_at":"2026-11-09T17:14:31.191Z","id":"arcl-c-01M70MVTCQBZR56XY3FVS3G84P","project_id":"arcl-pj-00000000000000000000000000","status":"captured","title":"Automate this"}}}
```

Check `format_version` before reading `data`. It changes when a field is
renamed, removed or retyped. New fields can appear without a new version, so
ignore fields you don't know. [Output formats and exit
codes](/reference/output/) lists which commands use which version.

## Find out why a task is not ready

`explain`, `show <task-id>` and `context` carry typed reasons in
`data.readiness`:

```sh
arcl --json explain AWEF | jq -c .data.readiness
```

```json
{"ready":false,"blocked":true,"reasons":[{"kind":"direct_blocker","message":"blocked by `arcl-t-01M70M8FPNTS24NBFPXW9F35WW` (pending)","blocker":"arcl-t-01M70M8FPNTS24NBFPXW9F35WW"}]}
```

Branch on `kind`, not on `message`. [Dependencies and readiness
rules](/reference/deps/#readiness-reasons) lists every kind and its fields.

## Chain from completed work

`task complete --json` returns `unblocked`, the tasks that were not ready
before the command and are ready after it, in `ready` order. A loop can run
`ready`, start a task, complete it, and start a task from `unblocked` next.

`task start` refuses a pending task that is not ready, exits `3`, and writes a
failure whose `message` lists the reasons as text. Read the typed reasons
from `explain` instead.

## Print one record per line

`--plain` prints one unstyled, tab-separated record per line:

```sh
arcl --plain tree
```

```text
0	spec	arcl-s-01M70M8AY4K0WT6RN8C4H4JKHQ	open	Add keyboard shortcuts
1	plan	arcl-pl-01M70M8FPNJTRS3HK1EZ7RDQ8G	open	Keyboard plan
2	phase	arcl-ph-01M70M8FPNEWXKRHYZS4B2GRYN	open	Key handling
3	task	arcl-t-01M70M8FPN8SMXD6TMX2G3MJS2	completed	Add the key bindings table
3	task	arcl-t-01M70M8FPNTS24NBFPXW9F35WW	pending	Move the selection with j and k
```

`--plain` and `--json` print full IDs.

## Print nothing on success

`--quiet` prints nothing on success, for a step whose exit code is all a
script needs:

```sh
arcl --quiet task complete KAJE
```

Errors still go to standard error.

## Handle failures

Under `--json`, a failure goes to standard error as a version 1 object:

```sh
arcl --json task start nope
```

```json
{"error":{"code":"not_found","exit_code":5,"message":"no record matches short ID `nope`"},"format_version":1}
```

Branch on the exit code or `error.code`. [Output formats and exit
codes](/reference/output/#exit-codes) lists them. Write full IDs in scripts,
since a short ID that is unique today can match a second record later.
