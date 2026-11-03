+++
id = "arcl-t-01M5YRFBR86YBZRSTWJRQBE0KB"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Refuse databases written by a newer arcl"
status = "completed"
priority = "normal"
position = 14
evidence = "Done on claude/schema-skew: the store refuses a database whose schema version is newer than it supports, and the workspace refuses a newer manifest, before any read or write. Both errors name the stored and supported versions and say to upgrade arcl. Store, repo and CLI tests cover a future-stamped database and manifest; the existing migration tests cover older databases."
+++

Stop with a clear message when the database or workspace was written by a
newer arcl, instead of failing on an unknown column or field. Taken from beads'
schema skew guard.

Scope:

- the schema version stored in the database and the workspace manifest
- a typed error naming both versions and how to upgrade
- a test that opens a database stamped with a future version

Acceptance criteria:

- [x] A database from a newer arcl is refused before any read or write
- [x] The error names the stored and supported versions
- [x] Older databases still migrate as before

Verification:

- store and CLI tests
- `just check`
