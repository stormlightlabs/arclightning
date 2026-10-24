+++
id = "arcl-c-01M5QZXF75KRDSCKYNDCW5V1TQ"
title = "Serve the Work list from one request"
status = "captured"
created-at = "2026-10-24T22:18:47.909Z"
+++

The Work view's `arcl serve` backend calls `/api/tasks/{id}/explain` once per task, since `/api/records` returns no priority, phase or readiness. One endpoint that returns tasks with their readiness and milestone would load the list in one request. Found in T18.2.
