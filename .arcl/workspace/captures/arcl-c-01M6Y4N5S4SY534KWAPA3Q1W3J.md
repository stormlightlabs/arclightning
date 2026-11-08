+++
id = "arcl-c-01M6Y4N5S4SY534KWAPA3Q1W3J"
title = "Add the assignee filter to the MCP list tool"
status = "captured"
created-at = "2026-11-08T17:52:47.396Z"
+++

`arcl list --assignee <name>` lists tasks claimed by a name. The MCP `list` tool takes the other `arcl list` filters through `RecordFilterArgs` but not `assignee`. Adding it is one input field passed to `RecordFilterArgs::assignee`. Found while adding the list filters to the MCP tool.
