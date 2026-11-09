---
title: Notes
description: Keep research, decisions and findings as Markdown notes linked to the records they describe.
---

A note is a Markdown record for research, decisions, meeting notes, debugging
logs and references. It has a title, a body and links to other records. A
note has no status and never affects readiness.

A note record differs from a task note. `arcl task note` appends a short,
timestamped line to one task, as the [tasks guide](/guides/tasks/#leave-notes)
shows. A note record stands on its own and can link to many records.

## Create a note

```sh
arcl note create "Keyboard research" --body "Vim-style keys suit list views."
```

```text
Created note `arcl-n-01M6Z4XMCZBNKAZFNW4S0HFWME`: Keyboard research
```

`--body-file <PATH|->` reads the body from a file or standard input. A
capture can also become a note with `arcl capture promote <id> note`.

## Link a note

Links connect a note to the records it is about. Each link names the
record's kind and ID:

```sh
arcl note link add FWME --kind spec --record-id FVMC
arcl note link add FWME --kind task --record-id 7TR6
arcl note link list FWME
```

```text
spec	arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC
task	arcl-t-01M6Z4WFW2QQWWHMFT4X4X7TR6
```

`--kind` is `capture`, `spec`, `plan`, `phase`, `task`, `note` or `release`,
and must match the record: a short ID is looked up only among records of that
kind. `arcl note link remove` takes the same options and removes one link.

The workspace file lists the links in its front matter:

```text
+++
id = "arcl-n-01M6Z4XMCZBNKAZFNW4S0HFWME"
title = "Keyboard research"

[[links]]
kind = "spec"
id = "arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC"

[[links]]
kind = "task"
id = "arcl-t-01M6Z4WFW2QQWWHMFT4X4X7TR6"
+++

Vim-style keys suit list views.
```

## Read and update notes

```sh
arcl note list
arcl note show FWME
arcl note update FWME --title "Keyboard research, round two" --body-file research.md
```

`arcl tree` counts notes in its footer, and `arcl tree --all` lists them under
a `Notes` heading. A note can join a release with `arcl release member add`.

[`arcl compact`](/guides/compaction/) writes one note per phase, titled
`Compacted work: <phase title>`, to keep the evidence of archived tasks.
