+++
id = "arcl-n-01M5P1YCNGYAHKA52NN6AE59TT"
title = "Decision: Existing bad edges load with a warning"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P14T2DD4DCF1P6X059E"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

Workspace sync and import refuse only blocker edge violations that the incoming workspace changes introduce, measured against the database graph. Violations already in the database load with a warning, as does every violation in a fresh import with nothing to compare against; T16.5 reports them in readiness and `arcl check`.

Decided 2026-10-24.
