+++
id = "arcl-n-01M5NH7Z1TRC4C3XCBADV2P4QF"
title = "Decision: Blocking rules"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

Inherited blockers; a task cannot block its own ancestor or descendant; readiness gates pending → in progress unless the caller passes `--force` (MCP does not expose force); cancelled blockers are reported as decisions to make; typed reasons; completions report what they unblocked.

Decided 2026-10-23.
