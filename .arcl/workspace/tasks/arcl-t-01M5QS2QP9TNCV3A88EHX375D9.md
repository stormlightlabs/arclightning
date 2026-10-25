+++
id = "arcl-t-01M5QS2QP9TNCV3A88EHX375D9"
parent = "arcl-t-01M5NE5P6XHRR4Q84820YACQ07"
title = "T18.5: Add the command menu and quick capture"
status = "completed"
priority = "normal"
position = 5
blocked-by = ["arcl-t-01M5QS2QJPGMYK0NE2MDRXN7QV"]
evidence = "Delivered by #42: a CommandMenu component in packages/ui on Dialog.svelte; the shortcut registry, menu scoping and quick capture with !priority, ^parent and after:blocker tokens in apps/web; POST /api/captures in crates/arcl-serve through the CLI's capture create with its write sync, and blocked_by on POST /api/tasks. The syntax is in Decision: Quick capture tokens and the manual. Verified with cargo test --workspace --all-features serve, pnpm --filter @arclightning/web check and test, the just check steps, a Playwright e2e run of the menu, shortcuts and capture, and a headless Chromium run against arcl serve on a scratch clone, confirmed with arcl task show and arcl capture show."
+++

Add the command menu and quick capture, so every action is a keystroke away.

Scope:

- a command menu component in `packages/ui/src/lib/components/`, built on
  `Dialog.svelte`
- a capture endpoint in `crates/arcl-serve`
- the shortcut registry in `apps/web`

Acceptance criteria:

- [x] `⌘K`, or `Ctrl+K` off macOS, opens a command menu that lists the actions
      for the current selection first, each with its shortcut
- [x] Choosing an action runs the same mutation as its button
- [x] A global shortcut opens quick capture from any view
- [x] Tokens in the capture text set fields on the capture; the syntax is
      recorded as a Decision note and in the manual
- [x] Every action in the app is reachable from the keyboard

Verification:

- `cargo test --workspace --all-features serve`: capture create
- `pnpm --filter @arclightning/web check`
- `pnpm --filter @arclightning/web test`: menu scoping, shortcuts, and token
  parsing
- `just check`

Out of scope:

- Triage of captures (T18.6)
