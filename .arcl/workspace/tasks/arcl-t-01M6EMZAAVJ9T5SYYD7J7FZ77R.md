+++
id = "arcl-t-01M6EMZAAVJ9T5SYYD7J7FZ77R"
phase = "arcl-ph-01M6EMZ9GEWCSG3Y6B41HBZ4HC"
title = "Push a project's plan to Linear"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M6EMZA7RNDXCQ1AQXGGJY60Q"]
+++

Push a project's plan to Linear.

Scope:

- `arcl linear push`: each open phase becomes a project milestone and each
  task an issue with title, body, priority, status and a link back to the
  record; new IDs are stored on the records
- subtasks become sub-issues and blockers become "blocks" relations
- `--dry-run` prints the changes; a second push with no arcl changes makes no
  API calls that change anything

Acceptance criteria:

- [ ] Pushing twice is idempotent
- [ ] Statuses map one way, documented in the manual
- [ ] Network and auth errors exit with clear messages and leave no partial IDs

Verification:

- tests against a recorded or fake Linear API
- `just check`
