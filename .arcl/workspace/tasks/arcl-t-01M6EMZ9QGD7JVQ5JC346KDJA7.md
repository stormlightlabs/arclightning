+++
id = "arcl-t-01M6EMZ9QGD7JVQ5JC346KDJA7"
phase = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"
title = "Serve every project's records with arcl hub serve"
status = "pending"
priority = "normal"
position = 1
blocked-by = ["arcl-t-01M6EMZ9KMSWQBQ3WHNWJ5KVMB"]
+++

Serve one read-only GraphQL schema across every registered project.

Scope:

- `arcl hub serve` binds `127.0.0.1` with the same Host and token checks as
  `arcl serve`, and a `--json` startup line
- `/graphql` answers with each project's records under a `projects` root
  field, every record carrying its project name; the per-project schema from
  arcl-graphql is reused, not copied
- each project's database is opened read-only per request; a project that
  fails to open is reported in the response and does not fail the others
- the same query limits as `arcl query`, counted across projects

Acceptance criteria:

- [ ] A query over two projects returns both, each record tagged with its project
- [ ] A broken project shows up as an error entry while the rest answer
- [ ] The hub never writes to a project's database or workspace

Verification:

- serve tests with two fixture projects and one broken one
- `just check`
