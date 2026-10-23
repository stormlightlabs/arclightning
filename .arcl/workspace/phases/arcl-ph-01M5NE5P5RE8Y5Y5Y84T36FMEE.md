+++
id = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 7: Local web app"
status = "open"
position = 7
+++

Serve the planning and execution workflow in the browser from the `arcl`
binary.

Deliverables:

- `arcl serve`, bound to `127.0.0.1` on an ephemeral port by default, with
  `--port` to pin one
- a `Host` header check and a per-run token in a request header on every
  request, an `Origin` check on mutations, no CORS headers, and no state
  changes on GET
- Inbox, Work, Planning, and Task views built from the shared UI package
- an HTTP adapter over the Rust application layer with the same validation and
  errors as the CLI
- Playwright workflow and screenshot coverage for the four views

Exit criterion: a developer can complete the primary planning and execution
workflow in the browser and observe the same results through the CLI
