+++
id = "arcl-c-01M5RFWETJCYSASCEPPP5VXJQP"
title = "Inbox does not list a quick capture filed while it is open"
status = "captured"
created-at = "2026-10-25T02:57:51.954Z"
+++

Quick capture from the open Inbox files the capture, but the list only shows it after the Inbox loads again (navigate away and back). The layout's oncaptured could ask the open view to reload. Found while writing apps/web/e2e/workflow.e2e.ts in T18.7, which captures from Work to avoid it.
