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

## Releases and Specs

Group spec-backed work into releases and epics:

```sh
arcl release create "Spring release" --description "Ship the next planning slice."
arcl epic create "Keyboard navigation" --spec specs/keyboard-navigation.md \
  --release arcl-r-01J...
arcl release update arcl-r-01J... --title "Updated spring release"
arcl epic update arcl-e-01J... --description "Refined scope"
```

Epic spec paths are resolved from the current directory and stored relative to the
Git worktree root.

The target must be an existing regular Markdown file inside the
worktree; absolute paths, `..` traversal, symlink escapes, duplicate specs, and
non-Markdown files are rejected.

Updating an epic changes tracker metadata only and never edits its linked spec.

Use `--no-release` to remove an epic's release association.

## Automation output

Mutations use concise output by default.

Use `--json` for a stable, versioned and machine readable output containing the record:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The JSON response has `format_version: 1`, an action, and the idea fields
(`id`, `title`, `description`, and `status`). `--plain` prints one idea ID per
line, while `--quiet` prints only the affected ID for mutations.
