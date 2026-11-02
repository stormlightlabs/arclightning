+++
id = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 13: Project hub"
status = "open"
position = 13
+++

One local place to see every project that uses arcl, without leaving the
repos. Each project keeps its own `.arcl/` and stays the only place its
records change; the hub reads them.

Deliverables:

- a user-level project registry (`arcl hub add`, `list`, `remove`)
- `arcl hub serve`: one local server and token over every registered project,
  with a merged read-only `/graphql`
- a hub dashboard built from the web app's components
- `arcl hub ready` and `arcl hub status` for the terminal

Exit criterion: with three registered projects, a developer sees their
milestones, ready work and in-progress work on one page and in one command,
and opens any record in its own project.
