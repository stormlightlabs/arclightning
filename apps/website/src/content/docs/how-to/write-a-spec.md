---
title: Write a spec
description: Create a spec with a Markdown body and acceptance criteria, edit it from the CLI or its workspace file, and close it when the work is done.
---

## Create a spec

Promote a capture, as [Capture and triage
work](/how-to/capture-and-triage/#promote-a-capture) shows, or create the spec
directly:

```sh
arcl spec create "Offline mode" \
  --body "Work without a network." \
  --acceptance-criteria "- [ ] Edits queue while offline"
```

```text
Created spec `arcl-s-01M6Z53YH37HKN8ZPXWCCY3J0G`: Offline mode
```

Both fields take Markdown. For longer text, `--body-file <PATH|->` and
`--acceptance-criteria-file <PATH|->` read from a file or standard input.

## Update a spec

```sh
arcl spec list
arcl spec show 3J0G
arcl spec update 3J0G --acceptance-criteria-file criteria.md
arcl spec update 3J0G --title "Offline editing"
```

With the workspace on, you can also edit `.arcl/workspace/specs/<id>.md`. The
metadata sits in TOML front matter and the body below it:

```text
+++
id = "arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC"
title = "Add keyboard shortcuts"
status = "open"
source-capture = "arcl-c-01M6Z4W77CXC4VZAJK35G1Y9K7"
acceptance-criteria = "- [ ] j and k move the selection"
+++

Support the common navigation actions.
```

The next `arcl` command imports the edit. [Workspace file
format](/reference/workspace-format/) lists the fields.

## Check what an agent reads

`arcl context <task-id>` shows the body of the spec above the task.
`arcl --json context <task-id>` adds the spec's `acceptance_criteria`, so an
agent reads the goal with the task.

## Close a spec

```sh
arcl spec complete 3J0G
arcl spec cancel 3J0G
```

A spec with open plans, phases or tasks below it refuses to close:

```text
error: invalid project or record: stored spec is invalid: cannot complete spec `arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC` while it has non-terminal descendants; pass --allow-open-children to override
```

Close the work below it first, or pass `--allow-open-children` to close only
the spec. Its open tasks then stay open but stop being ready.

Next, [plan the spec](/how-to/plan-from-a-file/).
