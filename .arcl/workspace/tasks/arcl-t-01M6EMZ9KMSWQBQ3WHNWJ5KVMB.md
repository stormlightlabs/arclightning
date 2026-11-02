+++
id = "arcl-t-01M6EMZ9KMSWQBQ3WHNWJ5KVMB"
phase = "arcl-ph-01M6EMZ9DHDAY6RXY3AKQT38ZK"
title = "Register projects with arcl hub add"
status = "pending"
priority = "normal"
position = 0
+++

Keep a list of the projects the hub reads, outside any one repo.

Scope:

- `arcl hub add [<path>]` (default: the current project), `arcl hub list`,
  `arcl hub remove <path|name>`
- the registry in the user config directory (`$XDG_CONFIG_HOME/arcl/hub.toml`
  or the platform equivalent), with a name per project defaulting to the
  directory name
- `list` reports a project whose path or `.arcl/` is gone, and does not drop it

Acceptance criteria:

- [ ] Adding the same project twice keeps one entry
- [ ] `arcl hub list --json` returns name, path and whether it is reachable
- [ ] A missing project is reported, not removed

Verification:

- CLI tests with a temporary config directory
- `just check`
