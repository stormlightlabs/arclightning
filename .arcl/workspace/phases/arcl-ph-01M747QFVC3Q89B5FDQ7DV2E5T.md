+++
id = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
plan-key = "hosted-serve"
title = "Milestone 16: Hosted serve"
status = "open"
position = 16
+++

Run `arcl serve` as its own service behind a gateway, as the spec "Run arcl
serve as a hosted service" (`arcl-s-01M747Q6A8X6GY5NBD1PN3MJ5Y`) describes.
Ships in v0.4.1.

Deliverables:

- `--bind`, refusing a non-loopback address without a supplied token
- a token from `--token-file` or `ARCL_SERVE_TOKEN` in both modes
- `--allow-host`, with `Origin` accepted for allowed hosts
- `GET /api/version` and `GET /healthz`
- `--project` to serve a directory other than the current one

Exit criterion: `arcl serve --bind 0.0.0.0 --port 7410 --token-file
/run/arcl/token --allow-host arcl:7410` serves the app and the API to a
client that sends the token and `Host: arcl:7410`, refuses one without the
token, and answers `GET /api/version`.
