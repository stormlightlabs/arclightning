+++
id = "arcl-t-01M5NH1M7KC96F91H2KMACN5AX"
phase = "arcl-ph-01M5NH1M0T75BYF0YNKHZXMYY3"
title = "T01: Support projects outside Git worktrees"
status = "completed"
priority = "normal"
position = 0
evidence = "Delivered by b7fca18 (feat: non-Git Arc project discovery), committed to main before pull requests."
+++

Allow Arc Lightning to initialize and discover a project in an ordinary
directory while preserving optional Git-aware behavior.

Blocked by: None - can start immediately

Acceptance criteria:

- [x] `arcl init` succeeds in a non-Git directory and creates a local project
- [x] Commands discover the nearest Arc Lightning project from descendant
      directories without relying on Git discovery
- [x] Git-backed projects still report repository state when repository-native
      mode uses it
- [x] Core domain and storage operations do not require a Git repository

Verification:

- `cargo test --workspace --all-features project_discovery`
- Complete init, idea, task, and ready operations in disposable Git and non-Git
  directories
