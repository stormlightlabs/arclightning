+++
id = "arcl-c-01M5NH405KDPY2486GVFGCV2SX"
title = "Order plan children by position in arcl tree"
status = "discarded"
created-at = "2026-10-23T23:21:44.371Z"
+++

`arcl tree` sorts a plan's phases by kind and ID (`plan_tree_node` in `crates/arcl-cli/src/app/query.rs`), ignoring `position`. `arcl plan show` and `arcl plan phase list` honor it. Phases created after others therefore list last: the Roadmap plan shows milestones 1, 2, 3 and 6 after milestone 10.
