+++
id = "arcl-t-01M5VDDD8HBGD5CTHESBV2SSQH"
phase = "arcl-ph-01M5VDD6PCB2B64MKW9TX01XXG"
title = "Leave closed captures out of arcl tree by default"
status = "pending"
priority = "normal"
position = 2
+++

Leave closed captures out of `arcl tree` by default. Promoted and discarded
captures print beside open ones today, so the tree lists more captures than
the Inbox does.

Add `--all` to include them. Promoted captures live on as the record they
became, and discarded ones are history, so neither belongs in the default
view of the plan.

Scope:

- leave out captures with status `promoted` or `discarded` by default, in
  human, `--plain` and `--json` output
- `arcl tree --all` to include them
- an explicit root ID still shows that record, closed or not

Acceptance criteria:

- [ ] `arcl tree` lists only open captures
- [ ] `arcl tree --all` lists every capture
- [ ] `arcl tree <closed capture ID>` shows it
- [ ] `--json` nodes keep their shape; only which captures appear changes

Verification:

- CLI tests with a captured, a promoted and a discarded capture, with and
  without `--all`
- on this repository, `arcl tree --plain | cut -f2 | grep -cx capture`
  equals `arcl list --kind capture --status captured --plain | wc -l`
- `just check`
