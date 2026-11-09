---
title: Keep research notes
description: Write research, decisions and findings as note records, and link each note to the specs, tasks and other records it describes.
---

A note record stands on its own and links to any number of records. For a
short line on one task, use `arcl task note`, as [Work a task to
done](/how-to/work-a-task/#record-notes-as-you-work) shows.

## Create a note

```sh
arcl note create "Keyboard research" --body "Vim-style keys suit list views."
```

```text
Created note `arcl-n-01M70MHBGM2E8VK7KEXTMMW4XR`: Keyboard research
```

`--body-file <PATH|->` reads the body from a file or standard input. To turn a
capture into a note, run `arcl capture promote <id> note`.

<div id="link-a-note"></div>

## Link it to records

Each link names the record's kind and ID:

```sh
arcl note link add W4XR --kind spec --record-id JKHQ
arcl note link add W4XR --kind task --record-id 7TR6
arcl note link list W4XR
```

```text
spec	arcl-s-01M70M8AY4K0WT6RN8C4H4JKHQ
task	arcl-t-01M6Z4WFW2QQWWHMFT4X4X7TR6
```

`--kind` is `capture`, `spec`, `plan`, `phase`, `task`, `note` or `release`.
A short `--record-id` matches only records of that kind.
`arcl note link remove` takes the same options and removes one link.

## Read and update notes

```sh
arcl note list
arcl note show W4XR
arcl note update W4XR --title "Keyboard research, round two" --body-file research.md
```

`arcl tree` counts notes in its footer, and `arcl tree --all` lists them under
a `Notes` heading.

## Ship a note with a release

```sh
arcl release member add SVY2C --kind note --record-id W4XR
```

[Cut a release](/how-to/cut-a-release/) covers releases.
