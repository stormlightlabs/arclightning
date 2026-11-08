+++
id = "arcl-c-01M6Y4N5N76Q7M7KQ78ZSQ02DV"
title = "Leave notes out of arcl list when a status filter is set"
status = "discarded"
created-at = "2026-11-08T17:52:47.271Z"
+++

With no `--kind`, `arcl list --status pending` lists every note beside the pending tasks. A note has no stored status (it reports `available`, which no filter accepts), and `list_records` in crates/arcl-store/src/storage/records.rs checks the status for every kind but notes. The MCP `list` tool shares that filter since #93, so it lists notes the same way. Found while adding the list filters to the MCP tool.

Fixed in #93.
