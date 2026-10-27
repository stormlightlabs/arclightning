+++
id = "arcl-t-01M5VDD6RKGADMNXF7KH4H0J83"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Make record IDs copyable in the web app"
status = "completed"
priority = "normal"
position = 10
evidence = "Delivered by #65: `CopyId` in packages/ui copies a record ID from its button or the ID itself, says Copied in a live region, and selects the ID when the clipboard is missing or refused. It sits on the IDs in the Task view, its properties panel, Work board cards and the Inbox detail. copy.e2e covers the serve build; artifact.e2e covers the artifact build with the clipboard write granted and refused against a stand-in host. The owner copied an ID in the published claude.ai artifact."
+++

Let a reader copy a record ID from the web app, so pasting it into
`arcl show`, a commit or a chat takes one click instead of a careful text
selection.

IDs appear in the Task view, its properties panel, Work board cards and the
Inbox detail. Each gets a copy button, and clicking the ID itself copies it
too. A short "Copied" confirmation follows, announced to screen readers.

`navigator.clipboard.writeText` needs a secure context and, inside the
claude.ai artifact iframe, a `clipboard-write` permission the host may not
grant. When the call is missing or refused, fall back to selecting the ID
text so the reader can copy it by hand, and say so instead of "Copied".

Scope:

- a copy control in `packages/ui`, with a copy icon added to `ICONS`
- the control on every record ID in the Task view, properties, board cards
  and Inbox detail
- the clipboard fallback for both builds

Acceptance criteria:

- [x] Each listed place shows a copy button beside the ID, with an
      accessible name such as "Copy ID"
- [x] Clicking the button or the ID copies the full ID and shows "Copied"
      in a live region
- [x] With no clipboard API, or a refused write, the ID is selected and the
      message says to copy it by hand
- [x] Copying works in the `arcl serve` build and the artifact build

Verification:

- a component test for the control, with the clipboard present, missing
  and refusing
- an e2e test that copies an ID in the serve build and reads the clipboard
- `pnpm --filter @arclightning/web build:artifact`, then copy an ID in the
  published artifact by hand
- `pnpm check`, `pnpm test`, `just check`
