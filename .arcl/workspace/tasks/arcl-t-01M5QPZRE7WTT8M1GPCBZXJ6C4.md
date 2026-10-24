+++
id = "arcl-t-01M5QPZRE7WTT8M1GPCBZXJ6C4"
parent = "arcl-t-01M5NE5P4D72Y56E0Y2JDE535B"
title = "T10.2: Install the skill with arcl skill install"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5QPZJRBMHJYTP3NVNHQ8T4Y"]
+++

Install the Arc Lightning skill from the binary, so a project gets it without
Node. `arcl skill install` writes the files from `skills/arclightning/` (T10.1)
into the project.

Scope:

- a `skill` command in `crates/arcl-cli/src/cli.rs` and
  `crates/arcl-cli/src/app/commands.rs`
- the skill files embedded with `include_str!` or `include_dir!` from
  `skills/arclightning/`, the one source directory, so the installed copy and
  the repository copy cannot diverge
- tests in `crates/arcl-cli/tests/cli.rs`
- the command in `apps/website/src/content/docs/reference/manual.md`

Acceptance criteria:

- [ ] `arcl skill install` writes the skill into the project's
      `.claude/skills/arclightning/` and `.agents/skills/arclightning/`
- [ ] Running it again leaves identical files unchanged
- [ ] `arcl skill install --check` writes nothing, reports each missing or
      changed file, and exits nonzero on drift
- [ ] The embedded files are read from `skills/arclightning/` at build time,
      not copied into the crate
- [ ] JSON, plain, and quiet output follow the other mutations

Verification:

- `cargo test -p arcl-cli --test cli skill_` (new tests, named `skill_…`):
  fresh install, repeat install, `--check` clean, `--check` after an edit, and
  `--check` with a missing file
- `just check`

Out of scope:

- Writing the skill text (T10.1)
- Agent directories other than `.claude` and `.agents`
