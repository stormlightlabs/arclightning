+++
id = "arcl-t-01M747RKZF3YMX5W36BVN3WJVW"
phase = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
title = "Accept configured hosts and origins in arcl serve"
status = "completed"
priority = "normal"
position = 3
evidence = "Done on `claude/serve-hosting`: repeatable `--allow-host`, checked by `allowed_host`; `Origin` accepted as `http://<Host>` or `http(s)://<allowed host>`. Tests `serve_checks_allowed_hosts`, `serve_accepts_an_allowed_host`, `serve_accepts_an_origin_naming_an_allowed_host`, `serve_answers_a_gateway_over_tcp`, `serve_answers_a_gateway_on_an_allowed_host`, `serve_refuses_a_malformed_allowed_host`; existing origin tests unchanged."
completed-at = "2026-11-11T02:53:00.911Z"
+++

Behind a Service the server is reached by name, such as `Host: arcl:7410`,
which gets 403 today.

Acceptance criteria:

- [ ] `--allow-host <host[:port]>` is repeatable and adds to the loopback hosts, compared with `Host` ignoring case
- [ ] A malformed value, such as one with a scheme or a path, fails the start
- [ ] A change is accepted when its `Origin` is `http://<Host>`, or `http://` or `https://` followed by an allowed host; any other `Origin` gets 403
- [ ] Without `--allow-host`, the checks are unchanged

Verification:

- Gate tests for allowed hosts and origins
- A CLI test with `--allow-host arcl:7410` that reads, queries `/graphql`, loads the app and makes a change with `Origin: http://arcl:7410`
- `just check`
