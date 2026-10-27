+++
id = "arcl-c-01M5Z32MJ1C30ZVERH6P1BX5YB"
title = "Share task claims across worktrees"
status = "captured"
created-at = "2026-10-27T16:28:43.969Z"
+++

A claim holds within one `.arcl/arcl.db`. Each Git worktree has its own database, so agents in separate worktrees can both claim one task; each sees the other's claim only after the workspace change reaches its checkout through Git and a sync. One option: store claims where every worktree reads them, such as a small database or lock file under the Git common dir (`git rev-parse --git-common-dir`), and check it on `task start --claim` alongside the task's own assignee. Raised in the adversarial review of PR #74.
