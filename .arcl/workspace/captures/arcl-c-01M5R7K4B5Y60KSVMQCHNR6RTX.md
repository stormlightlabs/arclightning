+++
id = "arcl-c-01M5R7K4B5Y60KSVMQCHNR6RTX"
title = "arcl serve: a huge declared Content-Length aborts the server"
status = "captured"
created-at = "2026-10-25T00:32:57.701Z"
+++

Found while revising #41. tiny_http 0.12 drains an unread body when a
`Request` drops, and `EqualReader::drop` allocates `vec![0; remaining]`
first. One request with `Content-Length: 1000000000000000`, with or
without the token, makes that allocation fail and aborts `arcl serve`
(reproduced: `memory allocation of 1000000000000000 bytes failed`).

tiny_http also hands a connection that arrives while its idle threads are
waking to a busy one, so a few stalled connections can hold up new ones,
reads included.

Neither can be fixed from `arcl-serve`'s side: tiny_http exposes no socket
and no read timeout, and every way to finish a request drops it. Options:
patch tiny_http (`[patch.crates-io]`), or move to another HTTP server.
