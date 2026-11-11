+++
id = "arcl-t-01M747RM2Z77GBTM4PQ8ZDZGDW"
phase = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
title = "Report the version at GET /api/version"
status = "completed"
priority = "normal"
position = 4
evidence = 'Done on `claude/serve-hosting`: `GET /api/version` answers `{"format_version":1,"data":{"version":"0.3.0"}}` today, behind the token and `Host` checks, GET only. Tests `serve_answers_the_version`, `serve_answers_a_gateway_on_an_allowed_host`.'
completed-at = "2026-11-11T02:53:01.023Z"
+++

The host checks which arcl it talks to.

Acceptance criteria:

- [ ] `GET /api/version` answers 200 `{"format_version":1,"data":{"version":"<v>"}}` with the crate version, matching `[0-9A-Za-z.+_-]{1,64}`
- [ ] It needs the token and `Host` like other API routes, takes GET only, and never redirects
- [ ] It answers under `--api` too

Verification:

- Server tests for the shape, the token check and the method
- `just check`
