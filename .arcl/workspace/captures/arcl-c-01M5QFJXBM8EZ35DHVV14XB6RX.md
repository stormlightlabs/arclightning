+++
id = "arcl-c-01M5QFJXBM8EZ35DHVV14XB6RX"
title = "Flaky subtask order in complete_with_subtasks test"
status = "captured"
created-at = "2026-10-24T17:33:24.724Z"
+++

`complete_with_subtasks_ignores_blockers_inside_the_subtree` in `execution.rs` failed 1 of 4 runs on main code: `completed_subtasks` came back with `first` and `second` swapped. The children share position 0, so tree order falls back to task ID, and IDs made in the same millisecond do not sort in creation order. Give the children distinct positions, or assert order-independent.
