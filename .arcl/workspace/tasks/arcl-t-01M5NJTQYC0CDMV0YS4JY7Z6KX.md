+++
id = "arcl-t-01M5NJTQYC0CDMV0YS4JY7Z6KX"
parent = "arcl-t-01M5NE5P2N1AMT731RJBAZ66YT"
title = "T17.3: Version and document the readiness JSON shapes"
status = "completed"
priority = "normal"
position = 3
blocked-by = ["arcl-t-01M5NJTQSVWEDN7GN2GB7TXBW5", "arcl-t-01M5NJTQWCZTH6DWQ3F9VDVX4Z"]
evidence = "Delivered on branch `claude/t17-3-version-docs`: the task view (`show`, `task show`, `explain`, `task explain`), context (`context`, `task context`) and `task complete` envelopes write `format_version: 2`; every other envelope, failures and warnings keep 1. The automation guide and manual list each reason `kind` with its ID fields, `completed_subtasks`, `unblocked`, the start refusal error, and the version per command. Tests: `output::tests` and `cli json_only_chain_picks_work_reads_reasons_and_starts_what_completion_unblocked`. Gates: cargo fmt --check, clippy -D warnings, cargo test --workspace, pnpm check, pnpm test, arcl check."
+++

Version the JSON shapes T17.1 and T17.2 changed and document them for the MCP
server (T10), then prove the milestone exit criterion with a JSON-only test.
Every envelope in `crates/arcl-cli/src/output.rs` writes `format_version: 1`
today, and `apps/website/src/content/docs/guides/automation.md` describes that
version.

Scope:

- `format_version` for the envelopes whose shape changed (readiness reasons,
  `task complete`), in `crates/arcl-cli/src/output.rs`
- `apps/website/src/content/docs/guides/automation.md` and
  `apps/website/src/content/docs/reference/manual.md`: readiness reason kinds
  and fields, the `unblocked` and `completed_subtasks` lists, the start
  refusal error, and which version each command returns
- an end-to-end test in `crates/arcl-cli/tests/cli.rs`

Acceptance criteria:

- [ ] Envelopes whose shape changed carry a new `format_version`; unchanged
      envelopes keep theirs
- [ ] The docs list every reason `kind`, its ID fields, the `unblocked` and
      `completed_subtasks` fields, and the version per command, in a form the
      MCP server can reuse
- [ ] A CLI test drives `ready`, `task start`, `task complete`, and `explain`
      with `--json` only: it picks ready work, reads why another task is
      blocked, completes a blocker, and starts a task from the `unblocked`
      list
- [ ] This PR also completes T17, with the T17 subtask PRs as its evidence

Verification:

- `cargo test -p arcl-cli output::tests` (existing output tests)
- `cargo test -p arcl-cli --test cli json_only_` (new test, named
  `json_only_…`): the end-to-end chain above
- `just check`
- Prettier on the changed docs

Out of scope:

- The MCP server (T10)
