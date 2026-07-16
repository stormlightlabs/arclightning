# Arc Lightning (`arcl`)

A Git-aware task tracking CLI for developers and their agents. Arc Lightning keeps
the live tracker in `.arcl/arcl.db` and can be used from any directory inside a
non-bare Git worktree.

## Quick start

Arc Lightning discovers the enclosing non-bare Git worktree, so initialization can
run from the repository root or any directory below it.

```sh
# From a new or existing Git worktree
arcl init

# Initialize with the version-controlled snapshot feature enabled
arcl init --snapshot
```

`init` creates the following structure in your project:

```sh
.arcl
├── .gitignore      # ignores sqlite, temp files & conflict artifacts
├── config.toml
└── arcl.db
```

## Ideating

Capture an idea in the local inbox:

```sh
arcl idea create "Add keyboard shortcuts" \
  --description "Support the most common navigation actions."
```

Descriptions can come from a UTF-8 Markdown file or piped in via stdin.

```sh
arcl idea create "Document recovery" --description-file notes.md
printf '# Recovery\n\nWrite down the restore steps.\n' \
  | arcl idea create "Document recovery" --description-file -
```

The command prints the generated ID, such as `arcl-i-01J...`. Use that ID to
edit, list, or discard the idea:

```sh
arcl idea update arcl-i-01J... --title "Document database recovery"
arcl idea update arcl-i-01J... --description-file notes.md
arcl idea list
arcl idea discard arcl-i-01J...
```

Discard is idempotent and soft-deletes it, preserving it as `discarded`; discarded ideas
cannot be updated or reopened.

## Automation output

Mutations use concise output by default.

Use `--json` for a stable, versioned and machine readable output containing the record:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The JSON response has `format_version: 1`, an action, and the idea fields
(`id`, `title`, `description`, and `status`). `--plain` prints one idea ID per
line, while `--quiet` prints only the affected ID for mutations.
