+++
id = "arcl-t-01M747RM9W8RWDG9N8RH30QAYQ"
phase = "arcl-ph-01M747QFVC3Q89B5FDQ7DV2E5T"
title = "Choose the project directory for arcl serve"
status = "completed"
priority = "normal"
position = 6
evidence = "Done on `claude/serve-hosting`: `--project <dir>` enters the directory before opening the database; no `--db`, since the checkout must be writable for workspace files and the sync lock, and `arcl.db` stays in `.arcl/`. `SPEC.md` says so. Test `serve_runs_in_the_project_directory`."
completed-at = "2026-11-11T02:53:01.269Z"
+++

A pod serves a checkout on a volume, not its working directory.

Acceptance criteria:

- [ ] `--project <dir>` serves the project that holds `dir`, as if run there
- [ ] A directory that does not exist fails the start with an error naming it
- [ ] `SPEC.md` says the checkout must be writable, since the web app writes workspace files and the sync lock under it, and that `arcl.db` stays in its `.arcl/`

Verification:

- A CLI test that serves a project from another working directory
- `just check`
