+++
id = "arcl-c-01M5R4X7HWSBW09BJ92M1N42YP"
title = "Store triage fields on captures"
status = "captured"
created-at = "2026-10-24T23:46:02.940Z"
+++

Quick capture (T18.5) parses `!priority`, `^parent` and `after:blocker` tokens, but a capture stores only a title and a body, so the tokens set fields only when the text is filed as a task. Store them on the capture (store, workspace format, sync, MCP) so `Enter` keeps them as fields and promotion (T18.6) applies them. See "Decision: Quick capture tokens".
