+++
id = "arcl-c-01M748B39WB4AAE9E7PCWA16YF"
title = "Document hosted arcl serve flags on the website"
status = "captured"
created-at = "2026-11-11T02:52:38.076Z"
+++

The CLI manual and the API guide in `apps/website/src/content/docs` (`reference/manual.md`, `guides/api.md`, `guides/rest.md`) still describe `arcl serve` as loopback-only. Add `--bind`, `--token-file`, `--allow-host`, `--project`, `GET /api/version`, `GET /healthz` and the gateway setup from `SPEC.md`. Wait for #98, which restructures those docs, to merge first.
