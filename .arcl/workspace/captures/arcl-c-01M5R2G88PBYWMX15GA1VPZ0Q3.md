+++
id = "arcl-c-01M5R2G88PBYWMX15GA1VPZ0Q3"
title = "Import shared crate items through their preludes in arcl-cli and arcl-serve"
status = "captured"
created-at = "2026-10-24T23:04:00.534Z"
+++

PR #38 added `arcl_core::prelude` and `arcl_store::prelude` and switched arcl-mcp to them. Switch the `use arcl_core::…` and `use arcl_store::…` lists in arcl-cli and arcl-serve to `use arcl_core::prelude::*;` and `use arcl_store::prelude::*;`, as AGENTS.md now asks. Where a crate glob-imports its own module with a same-named item (arcl-mcp hit `OpenSubtasks`), import that item explicitly.
