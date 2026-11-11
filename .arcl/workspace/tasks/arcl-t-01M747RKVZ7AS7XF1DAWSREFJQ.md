+++
id = "arcl-t-01M747RKVZ7AS7XF1DAWSREFJQ"
phase = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
title = "Bind arcl serve to a chosen address"
status = "completed"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M747RKR3EKZKVA46M448ES5B"]
evidence = "Done on `claude/serve-hosting`: `--bind <addr>` (default `127.0.0.1`) through `ServeOptions` and `Server::bind_with`, which returns `ServeError::OpenBind` off loopback without a supplied token. Tests `serve_refuses_to_mint_a_token_off_loopback`, `serve_binds_a_chosen_address_with_a_supplied_token`, `serve_refuses_to_bind_off_loopback_without_a_supplied_token`."
completed-at = "2026-11-11T02:53:00.782Z"
+++

`arcl serve` binds `127.0.0.1` only, so it cannot listen on the pod network.

Acceptance criteria:

- [ ] `--bind <addr>` takes an IPv4 or IPv6 address, default `127.0.0.1`
- [ ] A non-loopback address without a token from `--token-file` or `ARCL_SERVE_TOKEN` refuses to start, with exit 1 and an error that says why
- [ ] The printed URL names the bound address

Verification:

- A server test that `bind_with` refuses `0.0.0.0` without a token
- A CLI test that `arcl serve --bind 0.0.0.0` exits 1 without a token
- `just check`
