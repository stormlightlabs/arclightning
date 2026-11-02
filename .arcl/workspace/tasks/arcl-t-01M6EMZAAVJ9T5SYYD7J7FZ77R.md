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
- statuses map to the team's workflow states by state type (backlog,
  unstarted, started, completed, canceled), overridable in the status map
- each created issue's ID is written to its record as soon as Linear returns
  it, so a push that fails part way resumes on retry without duplicates
- the values each push sent are kept in a local, gitignored
  `.arcl/linear-state.json`; it does not survive a clone, and a push with no
  state file records Linear's current values as the baseline without reporting
- paged reads, and Linear's rate-limit headers respected with backoff
- `--dry-run` prints the changes; a second push with no arcl changes makes no
  API calls that change anything

Acceptance criteria:

- [ ] Pushing twice is idempotent
- [ ] Statuses map one way, documented in the manual
- [ ] A push stopped part way and run again creates no duplicate issues
- [ ] Network, auth and rate-limit errors exit with clear messages

Verification:

- tests against a recorded or fake Linear API
- `just check`
