+++
id = "arcl-t-01M6EMZADY4QYJVTW5F7CVZ4V0"
phase = "arcl-ph-01M6EMZ9GEWCSG3Y6B41HBZ4HC"
title = "Report edits made in Linear on push"
status = "pending"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M6EMZAAVJ9T5SYYD7J7FZ77R"]
+++

Report what people changed in Linear instead of overwriting it.

Scope:

- on push, compare each issue's title, status and assignee with the values
  in `.arcl/linear-state.json` and report differences per record
- `--accept-status` copies Linear statuses back as task transitions where the
  transition is allowed; comments come back as task notes with `--notes`
- nothing changes in arcl without one of those flags

Acceptance criteria:

- [ ] A status changed in Linear is reported on the next push
- [ ] With `--accept-status`, an allowed transition is applied and a refused
  one is reported with arcl's reason

Verification:

- tests against a recorded or fake Linear API
- `just check`
