+++
id = "arcl-t-01M747RKR3EKZKVA46M448ES5B"
phase = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
title = "Read the serve token from a file or the environment"
status = "completed"
priority = "normal"
position = 1
evidence = "Done on `claude/serve-hosting`: `--token-file` and `ARCL_SERVE_TOKEN` set the token in both modes, file first, trailing newline dropped. Unit tests `supplied_token_*` in `crates/arcl-cli/src/app/serve.rs`; CLI tests `serve_takes_its_token_from_a_file_before_the_variable`, `serve_refuses_a_bad_token_file`, `serve_page_mode_takes_the_token_variable`, `serve_api_refuses_a_bad_token_variable`."
completed-at = "2026-11-11T02:53:00.546Z"
+++

A hosted server needs the token the gateway holds, not one minted on each
start.

Acceptance criteria:

- [ ] `--token-file <path>` sets the token in the page mode and under `--api`
- [ ] `ARCL_SERVE_TOKEN` sets the token in both modes; the file wins when both are set
- [ ] The file's trailing newline is dropped
- [ ] A missing file, an empty or blank file, or a token that breaks the rule fails the start with exit 1 and an error naming the source, without echoing the token
- [ ] With neither source, each run mints a token, as before
- [ ] A supplied token is never printed; human output names its source

Verification:

- Unit tests for loading the token from a file, the environment, an empty file and a missing file
- CLI tests that start `arcl serve` with each source
- `just check`
