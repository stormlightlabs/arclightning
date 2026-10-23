+++
id = "arcl-c-01M5NJ0WW2WE3YB37EZFYCKQGT"
title = "Import an existing workspace during arcl init"
status = "captured"
created-at = "2026-10-23T23:37:31.266Z"
+++

On a fresh clone with a committed workspace, `arcl init` creates an empty database and leaves every record as `workspace added` until the next ordinary command syncs. `arcl workspace status` right after `init` shows 45 unsynced records. Consider importing during `init` when the workspace already holds records.
