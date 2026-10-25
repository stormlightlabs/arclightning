+++
id = "arcl-c-01M5S3PGWXN803Z0KB07EW59VV"
title = "Give the Inbox table a body excerpt column"
status = "captured"
created-at = "2026-10-25T08:44:08.989Z"
+++

The captures listing (`/api/records?kind=capture`, and `views/captures` in the artifact) carries only ID, title and status, so the Inbox table in apps/web shows title and created, with the date read from the ULID in the ID. A `created_at` field and a short body excerpt in that listing would let the table show both without a read per row. Found in T23.
