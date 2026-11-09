---
title: Install arcl
description: Install the arcl binary from a release, Homebrew, cargo-binstall or main, pin a version, or build it from source with the web app.
---

Release binaries cover Linux (x86_64 and arm64, static), macOS (arm64 and
x86_64) and Windows (x86_64). Each includes the web app that `arcl serve`
opens.

## Install a release

On macOS or Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-cli-installer.sh | sh
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-cli-installer.ps1 | iex"
```

With Homebrew:

```sh
brew install stormlightlabs/tap/arcl
```

The `latest` links skip pre-releases. Releases before 1.0 can change JSON
shapes; each envelope's `format_version` names the shape it carries.

## Install from main

The tutorials and several guides use commands that are on `main` and not in
a release: `arcl setup`, `arcl prime`, `arcl task note` and `--claim`, among
others. The latest release, v0.3.0, does not have them. Install `main` with
Cargo:

```sh
cargo install --git https://github.com/stormlightlabs/arclightning arcl-cli
```

The MCP server's `prime` tool is on `main` too. Install `arcl-mcp` the same
way:

```sh
cargo install --git https://github.com/stormlightlabs/arclightning arcl-mcp
```

Both need a Rust toolchain, and `arcl` built this way leaves out the web app,
as [Build from source](#build-from-source) explains. `main` carries version 0.3.0, so
`arcl --version` prints `arcl 0.3.0`, and `cargo binstall --git` downloads the
v0.3.0 release instead of building `main`.

## Pin a version

A cloud session or CI job should install a fixed version. Replace
`latest/download` in the installer URL with `download/<tag>`:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/download/v0.3.0/arcl-cli-installer.sh | sh
```

## Install with cargo-binstall

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall) downloads the
release binary instead of compiling it:

```sh
cargo binstall --git https://github.com/stormlightlabs/arclightning arcl-cli
```

`--git` takes the version from `main`. When `main` carries a version that is
not released yet, cargo-binstall finds no archive and compiles from source,
without the web app. To pin a release, and fail instead of compiling, install
from a clone of its tag:

```sh
git clone --depth 1 --branch v<version> https://github.com/stormlightlabs/arclightning
cargo binstall --manifest-path arclightning --disable-strategies compile arcl-cli
```

## Build from source

Building needs a Rust toolchain, plus Node and pnpm for the web app. Clone the
repository, build the web app, and install `arcl` into Cargo's bin directory:

```sh
git clone https://github.com/stormlightlabs/arclightning.git
cd arclightning
pnpm install
pnpm --filter @arclightning/web build
cargo install --path crates/arcl-cli
```

`arcl` embeds the web app from `apps/web/build` when it compiles, and that
build is not committed. Skip the `pnpm` steps and `arcl` still installs, but
`arcl serve` shows a page saying the web app was not built.

`cargo install --git` compiles a tag or branch without a checkout, and leaves
out the web app for the same reason:

```sh
cargo install --git https://github.com/stormlightlabs/arclightning --tag v0.3.0 arcl-cli
```

## Install the MCP server

`arcl-mcp` ships as its own binary. Each release has its own installers,
`arcl-mcp-installer.sh` and `arcl-mcp-installer.ps1`, but v0.3.0 has no
`prime` tool, so install it [from main](#install-from-main) for now. [Connect
the MCP server](/how-to/connect-mcp/) adds it to an agent.
