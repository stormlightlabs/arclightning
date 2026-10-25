+++
id = "arcl-c-01M5RB6WDGEHETAFS6RHF713K4"
title = "Let capture promotion to a task take blockers"
status = "captured"
created-at = "2026-10-25T01:36:10.672Z"
+++

Triage reads `after:REF` from a capture title, but `arcl capture promote` takes no blockers, so the Inbox only names them and asks for `arcl dependency add`. A `--blocked-by` option on promote, applied in the same transaction, would let the Inbox set them.
