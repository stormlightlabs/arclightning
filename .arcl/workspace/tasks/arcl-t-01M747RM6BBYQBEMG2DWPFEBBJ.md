+++
id = "arcl-t-01M747RM6BBYQBEMG2DWPFEBBJ"
phase = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
title = "Answer liveness at GET /healthz"
status = "completed"
priority = "normal"
position = 5
evidence = "Done on `claude/serve-hosting`: `GET /healthz` answers 200 `ok` with no token or `Host` check; other methods get 405 with `Allow: GET`. Tests `serve_answers_health_without_a_token_or_host`, `serve_answers_a_gateway_over_tcp`."
completed-at = "2026-11-11T02:53:01.139Z"
+++

Pod probes need a route that answers without the token.

Acceptance criteria:

- [ ] `GET /healthz` answers 200 with the body `ok`, with no token and any `Host`
- [ ] Other methods get 405 with `Allow: GET`
- [ ] It answers under `--api` too

Verification:

- Server tests for a request without a token or with a foreign `Host`
- `just check`
