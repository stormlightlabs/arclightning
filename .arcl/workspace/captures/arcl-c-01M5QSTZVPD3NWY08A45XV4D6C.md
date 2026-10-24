+++
id = "arcl-c-01M5QSTZVPD3NWY08A45XV4D6C"
title = "Add list filters for spec, plan, phase, parent, release and priority to the MCP list tool"
status = "captured"
created-at = "2026-10-24T20:32:35.190Z"
+++

The MCP `list` tool filters by kind and status only. `arcl list` also takes --spec, --plan, --phase, --parent, --release and --priority, with ancestry logic that lives in crates/arcl-cli/src/app/query.rs. Move that filter into the shared layer and expose it on the tool. Found in T10.3.
