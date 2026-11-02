+++
id = "arcl-c-01M6EW0NW3PV90RE6HYSME1SNG"
title = "Keep deep task trees from overflowing the stack"
status = "captured"
created-at = "2026-11-02T19:33:10.659Z"
+++

A parent chain about 2000 tasks deep overflows the stack in three reads. `arcl tree` crashes. `GET /api/tree` crashes the whole `arcl serve` process, so one deep tree takes the server down for every client. `arcl web export` fails sooner, at a depth of about 250.
