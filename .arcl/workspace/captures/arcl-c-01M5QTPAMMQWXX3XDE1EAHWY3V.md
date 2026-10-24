+++
id = "arcl-c-01M5QTPAMMQWXX3XDE1EAHWY3V"
title = "Decide whether arcl serve syncs the workspace"
status = "captured"
created-at = "2026-10-24T20:47:30.964Z"
+++

`arcl serve` reads the database without syncing the workspace, so a GET never writes and the server holds no sync lock. After a `git pull` the web app shows stale records until another arcl command syncs. Options: sync on start, sync under the lock per request, or a sync endpoint once mutations land (T18.4). Found in T18.1.
