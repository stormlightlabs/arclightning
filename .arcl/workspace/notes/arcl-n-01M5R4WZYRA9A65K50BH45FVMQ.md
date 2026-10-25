+++
id = "arcl-n-01M5R4WZYRA9A65K50BH45FVMQ"
title = "Decision: Quick capture tokens"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"

[[links]]
kind = "task"
id = "arcl-t-01M5QS2QP9TNCV3A88EHX375D9"
+++

Quick capture reads its text as fields. The first line is the title and the lines after it are the body. In the first line, three tokens set task fields:

- `!critical`, `!high`, `!normal` or `!low` sets the priority; the last one wins.
- `^REF` sets the parent task; the last one wins.
- `after:REF` adds a blocker, once for each token.

`REF` is a task ID, or the key a task title starts with followed by `:` or a space, such as `T18.4`. A reference that matches no task or more than one refuses the task. Only a whole word is a token, so `!later` and a bare `^` stay text, and the body is never read for tokens.

`Enter` files a capture and `⌘Enter`, or `Ctrl+Enter` off macOS, files a task. The tokens set fields only on the task: a capture stores only a title and a body, so it keeps the first line as typed, tokens included, for triage to read.

The token for a milestone, which the Inbox design shows as `^7`, waits for a way to name phases by number; `^` names the parent task meanwhile.

Decided 2026-10-24.
