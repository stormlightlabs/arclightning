+++
id = "arcl-c-01M5P0JB8RFBZ6KK26C2W7M44J"
title = "Refuse duplicate blockers in task create"
status = "captured"
created-at = "2026-10-24T03:51:43.128Z"
+++

`task create T --blocked-by A --blocked-by A` fails with a raw "UNIQUE constraint failed" error and exit code 1. `dependency add` refuses the same duplicate with DuplicateDependency and exit code 3, and `task create` should match it.
