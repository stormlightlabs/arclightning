+++
id = "arcl-c-01M5P0JBA26F3JG1SQXY5FN9SZ"
title = "Give rule refusals their own error"
status = "captured"
created-at = "2026-10-24T03:51:43.170Z"
+++

Refusals such as NotReady, OpenDescendants, and blocker edge violations print "invalid project or record: stored … is invalid", though nothing stored is invalid. Add a refusal variant without that prefix, and pin one full message in a CLI test.
