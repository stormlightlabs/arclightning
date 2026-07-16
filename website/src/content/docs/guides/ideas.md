---
title: Ideating
description: Capture and maintain early-stage work in the project inbox.
---

Use ideas for work that is worth recording before it has a release, epic, or
implementation plan. An idea keeps its title, Markdown description, generated
ID, and status in the local tracker.

## Capture an idea

```sh
arcl idea create "Add keyboard shortcuts" \
  --description "Support the most common navigation actions."
```

The command prints the generated ID, such as `arcl-i-01J...`. Descriptions are
Markdown and can be read from a UTF-8 file:

```sh
arcl idea create "Document recovery" --description-file notes.md
```

Use `--description-file -` to read from standard input:

```sh
printf '# Recovery\n\nWrite down the restore steps.\n' \
  | arcl idea create "Document recovery" --description-file -
```

## Review and update the inbox

List all ideas, then update the title or description by ID:

```sh
arcl idea list
arcl idea update arcl-i-01J... --title "Document database recovery"
arcl idea update arcl-i-01J... --description-file notes.md
```

The list includes the current status. New ideas are `captured`.

## Discard an idea

Discarding is a soft delete that preserves the record as `discarded`:

```sh
arcl idea discard arcl-i-01J...
```

Discard is idempotent. A discarded idea cannot be updated or reopened.
