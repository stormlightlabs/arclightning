+++
id = "arcl-c-01M5QZXFBZZ75CPJRQPJ3HH2BB"
title = "Keep option text out of Select's accessible name"
status = "discarded"
created-at = "2026-10-24T22:18:48.063Z"
+++

`Select.svelte` wraps the `<select>` in its `<label>`, so the control's accessible name includes every option's text, such as "Readiness AnyReadyBlockedWaiting". Label the select with only the label span. Found in T18.2.
