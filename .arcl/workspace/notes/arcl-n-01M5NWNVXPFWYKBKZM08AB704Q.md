+++
id = "arcl-n-01M5NWNVXPFWYKBKZM08AB704Q"
title = "Decision: Complete with subtasks"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

`task complete --with-subtasks` completes a task and its pending and in-progress descendants in one transaction. Completed and cancelled descendants stay as they are. A parked descendant, or an unfinished blocker from outside the subtree, refuses the command and nothing is written. Evidence goes on the task only. `--allow-open-children` keeps completing the task alone. T16.6 delivers it.

Decided 2026-10-24.
