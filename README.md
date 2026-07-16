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

## Planning milestones and tasks

Break an epic into ordered milestones, then add tasks and independently tracked subtasks:

```sh
arcl milestone create "Foundation" --epic arcl-e-01J... --position 10
arcl task create "Add schema" --milestone arcl-m-01J... --priority high --position 10 \
  --description "Create the tables needed by the tracker."
arcl task create "Add migration test" --milestone arcl-m-01J... \
  --parent arcl-t-01J... --description-file notes.md
```

Tasks and subtasks share the `arcl-t-<ulid>` identifier format. A subtask is a task
row with `parent_id`; Markdown checkboxes in descriptions remain prose. Update a
task's milestone to move its complete descendant subtree, or use `--parent` and
`--no-parent` to change its hierarchy. Parent and child rows must remain in the
same milestone, and cyclic reparenting is rejected before any rows change.

Milestone and task mutation commands support the same human, plain, quiet, and
versioned JSON output modes as the earlier entity commands.

## Automation output

Mutations use concise output by default.

Use `--json` for a stable, versioned and machine readable output containing the record:

```sh
arcl --json idea create "Automate this" --description "Details"
```

The JSON response has `format_version: 1`, an action, and the idea fields
(`id`, `title`, `description`, and `status`). `--plain` prints one idea ID per
line, while `--quiet` prints only the affected ID for mutations.
