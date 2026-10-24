+++
id = "arcl-n-01M5NWNVQZCCJN32QPP6DK7JRF"
title = "Decision: Parent completion"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

The PR that completes a parent task's last open subtask also completes the parent, with the subtask PRs as its evidence. To make that PR known in advance, the last subtask is blocked by its siblings and carries the criterion. Nobody completes a parent by hand.

Decided 2026-10-24.
