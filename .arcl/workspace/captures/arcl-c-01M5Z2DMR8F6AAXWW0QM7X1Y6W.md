+++
id = "arcl-c-01M5Z2DMR8F6AAXWW0QM7X1Y6W"
title = "Use immediate transactions for every read-check-write in arcl-store"
status = "captured"
created-at = "2026-10-27T16:17:16.040Z"
+++

PR #74 made task transitions and handoff take the write lock up front, so a racing claim gets `conflict` instead of SQLITE_BUSY. The other read-check-write paths in `storage/connected.rs`, `plan_ops.rs` and `promotions.rs` still open deferred transactions, so two processes writing at once can fail one with "database is locked" (exit 1) instead of waiting on the busy timeout.
