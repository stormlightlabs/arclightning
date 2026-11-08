+++
id = "arcl-c-01M6J6CWFK6DMDY29B4V07PT1C"
title = "Fail arcl check when a workspace file does not parse"
status = "discarded"
created-at = "2026-11-04T02:32:19.699Z"
+++

In arcl 0.3.0, `arcl check` exits 0 and prints `Check passed.` when a file
under `.arcl/workspace` cannot be parsed. It prints the parse error as a
warning, then checks the database as it was before the sync. A repository
that runs `arcl check` as a CI gate therefore passes a pull request that
breaks a task file. `arcl workspace import` rejects the same file with exit 3.

Found while reviewing stormlightlabs/stack#5, whose `just check` gate ran
`arcl check`.

## Reproduce

In any repository with the workspace enabled, set one task's status to a
value arcl does not accept:

```sh
sed -i 's/^status = .*/status = "nonsense"/' .arcl/workspace/tasks/<id>.md
rm -f .arcl/arcl.db*
arcl init --quiet          # exit 0
arcl check                 # exit 0
arcl workspace import --quiet   # exit 3
```

`arcl check` prints the TOML parse error (unknown variant `nonsense`) as a
`workspace_sync` warning, then `Check passed.` `arcl --json check` reports
`{"valid":true,"errors":[]}` with the error under `warnings`. The result is
the same when the database was built from a good tree first and the file
breaks afterwards. `arcl init --quiet` also exits 0 on the broken tree.

## Expected

`arcl check` fails, with a non-zero exit, when a workspace file does not
parse. The workspace is the record that git tracks, so a file arcl cannot
read is an invalid project, not a warning about the cache.

## Why it matters

The arclightning skill tells agents to run `arcl check` to validate the
project, and cloud's `just check` runs it as a gate. In CI the database is
always fresh from `arcl init`, so the sync warning is the only sign of the
broken file, and nothing reads it.

## Workaround in use

stack's `justfile` runs `arcl workspace import --quiet` before `arcl check`.

## Options

- Make `arcl check` exit non-zero when the workspace sync fails, and report
  the parse error under `errors`, not `warnings`.
- Keep the warning for commands that only read, but treat it as an error in
  `arcl check`, since checking is its job.
- Have `arcl init` fail, or warn, when the workspace does not import.

cloud (`scripts/check-tools.sh`) gates on `arcl check` and picks up the fix
with the next arcl release it pins.

Fixed in #89: `arcl check` fails with exit 3 and `workspace_invalid` when a workspace file does not parse.
