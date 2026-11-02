+++
id = "arcl-t-01M6EMZA7RNDXCQ1AQXGGJY60Q"
phase = "arcl-ph-01M6EMZ9GEWCSG3Y6B41HBZ4HC"
title = "Store Linear settings and IDs on records"
status = "pending"
priority = "normal"
position = 0
blocked-by = ["arcl-t-01M5NE5PD995CBF13DHR0A6HQR"]
+++

Store tracker settings per project and tracker IDs per record.

Scope:

- an `[integrations.linear]` table in `.arcl/config.toml` with `team` and
  `project`; the API key only from `LINEAR_API_KEY`, never written anywhere
- a `linear` field on phase and task workspace files holding the Linear ID,
  round-tripped by import and export
- the workspace format version raised only when a record carries the field

Acceptance criteria:

- [ ] A project without the table behaves as it does today
- [ ] The IDs survive a fresh clone and `arcl init`
- [ ] A config with a key in it is refused with a clear error

Verification:

- codec and CLI tests
- `just check`
