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
  field, every record carrying its project name. arcl-graphql exposes what
  this needs: running its schema against one database at a time with a node
  budget shared across the projects of one request
- each project's database is opened read-only, with no migrations and no
  workspace sync (a new read-only open in arcl-store). A project whose schema
  version differs from this arcl, or that fails to open, is reported in the
  response and does not fail the others
- each project reports whether its workspace has changes its database has not
  imported, using the comparison `arcl workspace status` makes, read-only
- the same depth, complexity and size limits as `arcl query`; the object cap
  counts across projects

Acceptance criteria:

- [ ] A query over two projects returns both, each record tagged with its project
- [ ] A broken project, or one on another schema version, shows up as an
  error entry while the rest answer
- [ ] The hub never writes to a project's database or workspace: a test
  compares file hashes before and after
- [ ] A project with unimported workspace changes is marked stale

Verification:

- serve tests with two fixture projects and one broken one
- `just check`
