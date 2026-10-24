+++
id = "arcl-c-01M5QXMQNGK7V4X2MPM86ZBR23"
title = "Host arcl-mcp as a claude.ai connector"
status = "captured"
created-at = "2026-10-24T21:39:04.496Z"
+++

Serve `arcl-mcp` over HTTP at `mcp.stormlightlabs.org`, so a claude.ai account can add it as a custom connector. A claude.ai connector needs a remote server behind sign-in; `arcl-mcp` runs over stdio on the user's machine. A hosted server needs rmcp's HTTP transport, GitHub OAuth so each person reaches only their own repositories, and a clone per repository that each call syncs, commits and pushes, with a race between pushes failing as `conflict`. It shares the "repository as backend" questions with T20. An idea for now, not planned.
