+++
id = "arcl-t-01M5VDDD42HZQ1FEN49X80GBA5"
phase = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
title = "Accept and show short record IDs"
status = "completed"
priority = "normal"
position = 0
evidence = "Delivered by #56: commands in the CLI and MCP server accept a unique ULID suffix (min 4 chars, optional kind such as t-535B) and refuse an ambiguous one with every match listed; show, list and tree print the shortest unique suffix in human and --plain output, JSON keeps full IDs; manual and inspect guide describe the rule. Verified with unit, CLI and MCP tests, arcl show GBA5 on this repo, and the just check steps."
+++

Show short record IDs in human output and accept them as input. Today
`arcl show 535B` fails with "unknown record ID", so every command needs the
full `arcl-<kind>-<ULID>`.

A ULID's first 10 characters are a millisecond timestamp, so records made
together share a long prefix: T16.1 to T17.3 all start `01M5NJTQ`. The
random tail tells them apart. A short ID is the shortest suffix of the ULID
that is unique among the project's records, at least 4 characters, as git
abbreviates SHAs.

Scope:

- compute short IDs in the shared application layer
- show them in `arcl tree`, `arcl list` and `arcl show` human and `--plain`
  output
- resolve a unique suffix, optionally with the kind (`t-535B`), wherever a
  record ID is accepted, in the CLI and the MCP server
- refuse an ambiguous suffix with an error that lists the matching IDs
- document the rule in the manual and the CLI reference

Acceptance criteria:

- [ ] `arcl show 535B` shows the record whose ULID ends in `535B`
- [ ] `arcl show t-535B` limits the match to tasks
- [ ] An ambiguous suffix fails with every matching full ID listed
- [ ] Human output shows the shortest unique suffix, at least 4 characters
- [ ] `--json` output keeps full IDs and its current shape
- [ ] The manual and the CLI reference describe short IDs

Verification:

- unit tests for the shortest-unique-suffix rule and suffix resolution,
  including ambiguity and a kind prefix
- CLI tests for `arcl show`, `arcl task start` and `arcl tree` with a
  short ID
- `arcl show <suffix>` on this repository
- `just check`
