+++
id = "arcl-c-01M5Z4R5ZV2SRPC4FHRACG6D67"
title = "Keep a +++ line in Markdown fields from breaking task files"
status = "captured"
created-at = "2026-10-27T16:57:58.523Z"
+++

A handoff, evidence, body or other multi-line front matter string with a line that is exactly `+++` is written into a TOML multi-line string as is, so the line closes the front matter early. The next import fails with a TOML parse error. Found while adding task notes, which refuse such a line instead. Escape the line when rendering (for example `\u002B++` in a basic string) or refuse it on input for every field.
