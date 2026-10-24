+++
id = "arcl-c-01M5NZMECCW6ADCQABBXYJRJMP"
title = "Validate only the new blocker edges on each write"
status = "captured"
created-at = "2026-10-24T03:35:23.276Z"
+++

The whole-graph check costs about 323 ms per write at a 2,000-task chain. A per-edge check that starts from the new edge would cost about 1 ms.
