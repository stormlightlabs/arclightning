+++
id = "arcl-n-01M5NH7Z7F7FRZXCF5AFDYR66N"
title = "Decision: Local web app security"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

`arcl serve` binds 127.0.0.1 on an ephemeral port by default and `--port` pins one. The token travels in a request header, mutations need a matching `Origin`, the `Host` check stops DNS rebinding, there are no CORS headers, and GET never changes state.

Decided 2026-10-23.
