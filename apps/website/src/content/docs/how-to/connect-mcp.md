---
title: Connect the MCP server
description: Install arcl-mcp and add it to Claude Code or another MCP client, so an agent works through typed tools instead of CLI output.
---

`arcl-mcp` gives an agent each command's JSON envelope as structured content,
with an input and an output schema per tool and no terminal text to parse.

<div id="install-and-connect"></div>

## Install arcl-mcp

On macOS or Linux:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/latest/download/arcl-mcp-installer.sh | sh
```

On Windows, run `arcl-mcp-installer.ps1` from the same release, as [Install
arcl](/how-to/install/#install-a-release) shows for `arcl`. To compile it:

```sh
cargo install --git https://github.com/stormlightlabs/arclightning arcl-mcp
```

## Add it to Claude Code

```sh
claude mcp add arcl -- arcl-mcp
```

The client starts `arcl-mcp` in the project directory. The server serves the
project that holds its working directory, and takes no arguments.

## Add it to another client

Configure the client to run the command `arcl-mcp`, with no arguments, from
the project directory. Any MCP client that can start a command over standard
input and output can use it.

## Pair it with the skill

Run [`arcl setup`](/how-to/set-up-an-agent/) as well. The skill it writes
tells the agent to use the tools when the server is connected, and names the
tool for each step of the loop. The session hook still runs `arcl prime`, and
the server's instructions tell the client to call the `prime` tool at the
start of a session.

<div id="how-tools-run"></div>
<div id="tools"></div>
<div id="prime"></div>
<div id="pages-and-filters"></div>
<div id="starting-tasks"></div>

## Check that it works

Ask the agent to call `prime`. It returns the working loop and the open and
ready work. Outside an initialized project it returns `data.initialized`
false and a message that says to run `arcl init`.

[MCP tools](/reference/mcp/) lists every tool and its arguments.
