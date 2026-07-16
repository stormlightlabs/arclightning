---
title: Automation
description: Consume Arc Lightning mutations from scripts and agents.
---

Arc Lightning keeps mutation output short by default. Choose an explicit output
mode when a script or coding agent needs a stable interface.

## Use versioned JSON

Pass `--json` before or after the command:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The response is a versioned JSON object with `format_version: 1`, an action, and
the affected record fields. For an idea, those fields include `id`, `title`,
`description`, and `status`.

Use JSON when the consumer needs named fields rather than text intended for a
terminal.

## Use plain or quiet output

`--plain` emits one stable, unstyled record per line:

```sh
arcl --plain idea list
```

`--quiet` suppresses explanatory output and prints only the affected ID for a
mutation:

```sh
arcl --quiet task complete arcl-t-01J...
```

These modes keep shell pipelines and agent tool calls free of terminal styling
and incidental status messages.

## Keep failures machine-readable

JSON mode writes structured failures to standard error. Use the exit status to
distinguish a successful command from a parsing error, invalid project or
record, storage failure, or missing record. See the [manual](/reference/manual/)
for the complete output and exit-status reference.
