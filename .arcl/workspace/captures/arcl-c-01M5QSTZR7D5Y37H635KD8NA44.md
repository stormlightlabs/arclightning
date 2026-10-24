+++
id = "arcl-c-01M5QSTZR7D5Y37H635KD8NA44"
title = "Share project discovery and workspace sync between the CLI and MCP"
status = "captured"
created-at = "2026-10-24T20:32:35.079Z"
+++

arcl-mcp's `project` module repeats the CLI's project-root lookup, config read, sync lock and auto-sync around a call (crates/arcl-cli/src/app/auto_sync.rs, support.rs). Move them into a shared crate, such as arcl-repo, so both adapters use one copy and cannot drift. Found in T10.3.
