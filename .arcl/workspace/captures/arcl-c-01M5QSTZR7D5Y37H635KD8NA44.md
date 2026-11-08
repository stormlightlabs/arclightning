+++
id = "arcl-c-01M5QSTZR7D5Y37H635KD8NA44"
title = "Share project discovery and workspace sync between the CLI and MCP"
status = "discarded"
created-at = "2026-10-24T20:32:35.079Z"
+++

arcl-mcp's `project` module repeats the CLI's project-root lookup, config read, sync lock and auto-sync around a call (crates/arcl-cli/src/app/auto_sync.rs, support.rs). Move them into a shared crate, such as arcl-repo, so both adapters use one copy and cannot drift. Found in T10.3.

Fixed in this PR: the shared `arcl_repo::project` module finds the project, opens its database and runs the sync lock and the syncs for both the CLI and the MCP server.
