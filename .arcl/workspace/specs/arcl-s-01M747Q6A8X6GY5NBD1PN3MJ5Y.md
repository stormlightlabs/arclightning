+++
id = "arcl-s-01M747Q6A8X6GY5NBD1PN3MJ5Y"
title = "Run arcl serve as a hosted service"
status = "open"
acceptance-criteria = """
- [ ] `arcl serve --bind 0.0.0.0 --port 7410 --token-file /run/arcl/token --allow-host arcl:7410` serves the app and the API to a client that sends the token and `Host: arcl:7410`
- [ ] The same server refuses a request without the token with 401, and one with another `Host` with 403
- [ ] `GET /api/version` answers 200 with `data.version`
- [ ] `GET /healthz` answers 200 without a token
- [ ] `--bind` off loopback without a supplied token refuses to start
- [ ] `SPEC.md` and `arcl serve --help` describe the flags"""
+++

A host wants to run `arcl serve` in a Kubernetes pod behind its own gateway.
The gateway handles identity, strips the caller's credentials and sends a
token in `X-Arcl-Token` on each request it forwards. `arcl serve` binds
`127.0.0.1` only, mints a token on each start in the page mode, and accepts
only `Host: 127.0.0.1:<port>` or `localhost:<port>`. So today it can run only
in the gateway's network namespace, and the two cannot be deployed or
restarted separately.

## Needed

1. A bind address: `--bind <addr>`, default `127.0.0.1`.
2. A fixed token from a file or the environment. A missing or empty token
   fails the start.
3. Allowed hosts: `--allow-host <host[:port]>`, repeatable, added to the
   loopback defaults.
4. A rule for `Origin` behind a gateway.
5. `GET /api/version`, answering the version as `data.version`.

## Nice to have

- `GET /healthz` without a token, for the pod's probes.
- Container-friendly paths: the project directory to serve, and where
  `arcl.db` lives.

## Decisions

- `--bind` takes an IP address, default `127.0.0.1`. A non-loopback address
  needs a token from `--token-file` or `ARCL_SERVE_TOKEN`; without one
  `arcl serve` refuses to start. A minted token is printed to the log, so it
  never guards a server off loopback.
- `--token-file <path>` and `ARCL_SERVE_TOKEN` set the token in both modes.
  The file wins when both are set. The file's trailing newline is dropped. An
  empty file, a missing file or a token that breaks the rule (at least 32
  characters, ASCII letters and digits only) fails the start with exit 1 and
  an error that names the source and never echoes the token. With neither
  source on loopback, each run mints a token, as before. A supplied token is
  never printed.
- The token travels in `X-Arcl-Token`, as it does today.
- `--allow-host` values are compared with `Host` exactly, ignoring case.
  `arcl:7410` allows `Host: arcl:7410` only; pass `arcl` as well for a
  gateway that drops the port.
- No `--allow-origin`. A change is accepted when its `Origin` is
  `http://<Host>`, as today, or `http://` or `https://` followed by an
  `--allow-host` value. A gateway that terminates TLS for
  `https://arcl.example.com` adds `--allow-host arcl.example.com`.
- `GET /api/version` answers `{"format_version":1,"data":{"version":"<v>"}}`
  with the crate version. It needs the token and `Host` like every API
  route, and takes GET only.
- `GET /healthz` answers 200 `ok` with no token and no `Host` check, since a
  probe sends the pod IP as `Host` and the answer reveals nothing. Other
  methods get 405.
- `--project <dir>` serves the project at `dir` instead of the current
  directory. There is no `--db`: the web app writes workspace files and the
  sync lock under the project, so the checkout must sit on a writable volume,
  and `arcl.db` stays in its `.arcl/` beside them. arcl keeps no cache.
- Everything ships in v0.4.1.

## Answers to the host

- Workspace source: a Git checkout of the repository on a writable volume,
  passed as `--project`. Run `arcl init` in it once before `arcl serve`; it
  builds `arcl.db` from `.arcl/workspace`. Reads do not sync, so after
  pulling run `arcl workspace sync`.
- Commits: `arcl serve` writes workspace files under `.arcl/workspace` but
  never commits. A sidecar or job commits and pushes them;
  `arcl workspace status` lists the changed files.
- Origin: no `--allow-origin`. The gateway sends an `Origin` naming an
  allowed host.
- Release: all five needed items and both nice-to-haves ship in v0.4.1.
