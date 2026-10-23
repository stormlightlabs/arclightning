+++
id = "arcl-ph-01M5NH1M2H5JPEW4DMNF5RMDQB"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 2: Shared Rust application"
status = "completed"
position = 2
+++

Extract the connected planning operations into reusable crates once their
responsibilities are established by working behavior.

Deliverables:

- core domain and application operations independent of delivery interfaces
- separate SQLite and repository-file responsibilities
- a CLI adapter that preserves the `arcl` binary
- removal of duplicate domain paths after migration

Exit criterion: the CLI passes through the shared Rust application layer, and
new adapters can use the same operations without invoking the CLI
