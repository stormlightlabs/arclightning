+++
id = "arcl-t-01M5QS2QP9TNCV3A88EHX375D9"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.5: Add the command menu and quick capture"
status = "in_progress"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5QS2QJPGMYK0NE2MDRXN7QV"]
+++

Add the command menu and quick capture, so every action is a keystroke away.

Scope:

- a command menu component in `packages/ui/src/lib/components/`, built on
  `Dialog.svelte`
- a capture endpoint in `crates/arcl-serve`
- the shortcut registry in `apps/web`

Acceptance criteria:

- [ ] `⌘K`, or `Ctrl+K` off macOS, opens a command menu that lists the actions
      for the current selection first, each with its shortcut
- [ ] Choosing an action runs the same mutation as its button
- [ ] A global shortcut opens quick capture from any view
- [ ] Tokens in the capture text set fields on the capture; the syntax is
      recorded as a Decision note and in the manual
- [ ] Every action in the app is reachable from the keyboard

Verification:

- `cargo test --workspace --all-features serve`: capture create
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`: menu scoping, shortcuts, and token
  parsing
- `just check`

Out of scope:

- Triage of captures (T18.6)
