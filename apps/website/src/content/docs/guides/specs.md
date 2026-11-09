---
title: Specs
description: Write the goals Arc Lightning owns, with a Markdown body and acceptance criteria, and close them when the work is done.
---

A spec states a goal: what should be true when the work is done. Arc Lightning
owns the spec's content. Its title, Markdown body and acceptance criteria live
in the record, and with the workspace on, in `.arcl/workspace/specs/<id>.md`.
Plans and tasks hang below it.

## Create a spec

Promote a capture, as the [captures guide](/guides/captures/#promote-a-capture)
shows, or create one directly:

```sh
arcl spec create "Offline mode" \
  --body "Work without a network." \
  --acceptance-criteria "- [ ] Edits queue while offline"
```

```text
Created spec `arcl-s-01M6Z53YH37HKN8ZPXWCCY3J0G`: Offline mode
```

Both fields take Markdown. `--body-file <PATH|->` and
`--acceptance-criteria-file <PATH|->` read them from a file or standard input.

The workspace file holds the same content, with the metadata as TOML front
matter and the body below it:

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

You can edit that file and let the next command import it. The
[workspace guide](/guides/workspace/#edit-workspace-files) has the rules.

## Read and update a spec

```sh
arcl spec list
arcl spec show FVMC
arcl spec update FVMC --acceptance-criteria-file criteria.md
arcl spec update FVMC --title "Keyboard navigation"
```

`arcl context <task-id>` shows the body of the spec above the task, and
`arcl --json context` adds its `acceptance_criteria`, so an agent working the
task reads the goal with it.

## Close a spec

```sh
arcl spec complete FVMC
arcl spec cancel FVMC
```

A spec with open plans, phases or tasks below it refuses to close:

```text
error: invalid project or record: stored spec is invalid: cannot complete spec `arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC` while it has non-terminal descendants; pass --allow-open-children to override
```

`--allow-open-children` closes only the spec. Its open tasks stay open but
stop being ready, since a task under a closed spec cannot start. The
[lifecycle guide](/guides/lifecycle/) covers the rules every container
follows.

## Write the plan next

A [plan](/guides/plans/) describes how to build the spec. A spec can have
more than one plan, such as a first attempt and its replacement.
