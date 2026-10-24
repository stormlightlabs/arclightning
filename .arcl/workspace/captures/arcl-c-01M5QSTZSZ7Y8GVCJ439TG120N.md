+++
id = "arcl-c-01M5QSTZSZ7Y8GVCJ439TG120N"
title = "Return the failure envelope for MCP argument errors"
status = "discarded"
created-at = "2026-10-24T20:32:35.135Z"
+++

When a tool call's arguments do not deserialize (an unknown field such as `force` on `task_start`, or a wrong type), rmcp returns its own text error result, not the `{format_version: 1, error: {code, message, exit_code}}` object the tools return for other failures. Wrap parameter extraction so these return `invalid_request` in the envelope. Found in T10.3; may fit T10.4's error coverage.

Fixed in T10.3 (PR #34): `arcl_mcp::params::Parameters` returns argument errors as `invalid_request`.
