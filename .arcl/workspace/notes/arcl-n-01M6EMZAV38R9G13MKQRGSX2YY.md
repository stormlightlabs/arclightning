+++
id = "arcl-n-01M6EMZAV38R9G13MKQRGSX2YY"
title = "Decision: a local hub first, trackers as opt-in mirrors"

[[links]]
kind = "phase"
id = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"

[[links]]
kind = "phase"
id = "arcl-ph-01M6EMZ9GEWCSG3Y6B41HBZ4HC"
+++

Each repo keeps its own `.arcl/` and stays the only place its records change.
Cross-project views read; they never write.

- The arcl hub comes first, before 1.0. It is local and needs no account:
  a user-level registry, `arcl hub serve` with one token and a merged
  read-only `/graphql` built on arcl-graphql, a dashboard and terminal views.
  The desktop app will share most of its views.
- External trackers are opt-in mirrors. arcl owns planning; a tracker gets a
  push-mostly copy. Edits made in the tracker are reported, and come back only
  with an explicit flag. Linear is the first adapter, after the desktop app.
- Tracker IDs live on workspace records so the mapping survives a clone. API
  keys come from the environment and are never stored.

Decided 2026-11-02.
