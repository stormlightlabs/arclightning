+++
id = "arcl-t-01M61HW51Z6X4YHYAB35AYZDQ8"
phase = "arcl-ph-01M5YRFB35TZ51QW4AG1A8MS5B"
title = "Serve only the API with arcl serve --api"
status = "pending"
priority = "normal"
position = 12
blocked-by = ["arcl-t-01M61HW4VBTNF4G8PEJ6G996Z4"]
+++

`arcl serve --api` serves `/graphql` and `/api` without the web page, so a
script or another local tool can start the API against the project's
database.

Scope:

- `--api` skips the page: any path outside `/api` and `/graphql` gets 404
- with `--api`, the token from `ARCL_SERVE_TOKEN` when it is set: at least
  32 characters, letters and digits only, refused at startup otherwise;
  without it, a new one per run. The page mode always makes a new one
- `--json` prints one startup line with the URL, port and token
- with `--api`, a write needs a matching `Origin` only when the request
  sends one; the token header still blocks other pages, since a browser
  cannot send it cross-origin without a CORS preflight the server never
  answers. The page mode keeps requiring `Origin`
- still bound to `127.0.0.1`

Acceptance criteria:

- [ ] `arcl serve --api` answers `/graphql` and `/api` and returns 404 for `/`
- [ ] A client without an `Origin` header can run a mutation with the token
- [ ] A mutation with a mismatched `Origin` is still refused
- [ ] A bad `ARCL_SERVE_TOKEN` stops startup with an error naming the rule
- [ ] The manual and SPEC.md "Local web app" document `--api`,
  `ARCL_SERVE_TOKEN`, the startup line and the `Origin` rule

Verification:

- serve tests for each acceptance criterion
- `just check`
