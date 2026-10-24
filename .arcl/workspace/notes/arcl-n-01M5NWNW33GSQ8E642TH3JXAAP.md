+++
id = "arcl-n-01M5NWNW33GSQ8E642TH3JXAAP"
title = "Decision: Starting a parent"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

`task start` without `--force` skips only the "has child tasks" readiness reason. A parent starts when it and its ancestors have no unfinished blockers and no other reason applies. T16.2 delivers it.

Decided 2026-10-24.
