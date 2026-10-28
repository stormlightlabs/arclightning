+++
id = "arcl-t-01M5YRFBR86YBZRSTWJRQBE0KB"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Refuse databases written by a newer arcl"
status = "pending"
priority = "normal"
position = 14
+++

Stop with a clear message when the database or workspace was written by a
newer arcl, instead of failing on an unknown column or field. Taken from beads'
schema skew guard.

Scope:

- the schema version stored in the database and the workspace manifest
- a typed error naming both versions and how to upgrade
- a test that opens a database stamped with a future version

Acceptance criteria:

- [ ] A database from a newer arcl is refused before any read or write
- [ ] The error names the stored and supported versions
- [ ] Older databases still migrate as before

Verification:

- store and CLI tests
- `just check`
