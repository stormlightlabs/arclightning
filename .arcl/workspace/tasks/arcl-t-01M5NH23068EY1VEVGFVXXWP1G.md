+++
id = "arcl-t-01M5NH23068EY1VEVGFVXXWP1G"
phase = "arcl-ph-01M5NH1M0T75BYF0YNKHZXMYY3"
title = "T03: Implement capture promotion and owned planning content"
status = "completed"
priority = "normal"
position = 2
blocked-by = ["arcl-t-01M5NH22V9CABE81P6CPCEEQ4H"]
evidence = "Delivered by d6728f8 (feat: idempotent plan apply and capture promotion), committed to main before pull requests."
+++

Let users create and edit the new records and move a capture into a spec, task,
or note without losing provenance.

Blocked by: T02

Acceptance criteria:

- [x] Create, show, update, list, and transition captures, specs, plans, phases,
      and notes through application operations
- [x] Promote a capture transactionally to a spec, task, or note
- [x] Make repeated promotion to the same target idempotent and reject an
      ambiguous second target
- [x] Check, diff, and apply structured plan input against a persistent plan
      without duplicating tasks on repeated application
- [x] Return validation errors without partial writes

Verification:

- `cargo test --workspace --all-features capture`
- `cargo test --workspace --all-features plan`
- Exercise create, edit, promotion, plan diff, and repeated plan apply through
  the application API
