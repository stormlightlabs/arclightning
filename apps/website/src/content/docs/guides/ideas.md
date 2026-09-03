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

## Promote Ideas

Once an idea has an implementation spec, promote it into an epic:

```sh
arcl idea promote arcl-i-01J... --spec specs/keyboard-navigation.md
```

Use `--release <ID>` to place the new epic in an existing release. Arc Lightning
creates the epic, links it to the source idea, and marks the idea promoted in one
transaction. The idea title and description become the initial epic title and
description.

Repeating the same promotion returns the existing epic. A discarded idea cannot
be promoted. The spec must be an existing Markdown file inside the worktree.

Use `arcl show` on either record to follow the relationship:

```sh
arcl show arcl-i-01J...
arcl show arcl-e-01J...
```
