+++
id = "arcl-t-01M5NH1N3R9AX3XZH6XG9ZX1QE"
phase = "arcl-ph-01M5NH1M603NDS40YKKA2R150W"
title = "T11: Create the pnpm workspace and shared UI package"
status = "completed"
priority = "normal"
position = 0
evidence = """
Delivered by commits to main before pull requests:

- d18d827 (build: scaffold kit desktop app)
- 4932948 (feat: migrate ui to sveltekit and create shared pkg)"""
+++

Build the reusable Svelte component and token package consumed by the desktop
app and website.

Blocked by: None - can start immediately

Acceptance criteria:

- [x] Move the existing `website` directory to `apps/website` and update root
      scripts, package references, and development commands without redesigning
      the site in this task
- [x] Configure a pnpm workspace for `packages/ui`, `apps/desktop`, and
      `apps/website` without duplicating frontend dependencies
- [x] Define semantic light and dark tokens using navy/sky blue with
      gold/amber/yellow accents
- [x] Add accessible primitives for buttons, fields, menus, dialogs, tabs,
      status, priority, feedback, and Markdown display
- [x] Add planning components needed by both consumers, with application data
      supplied through props and events
- [x] Keep Tauri APIs, routing, persistence, and website content out of the
      package
- [x] Provide a component review surface and tests for state, keyboard use,
      focus, theme, and reduced motion
- [x] Configure Vitest unit and browser projects with
      `@vitest/browser-playwright`, Chromium, and `vitest-browser-svelte`
- [x] Use `page` locators and accessible role, label, or text queries in browser
      component tests
- [x] Add Playwright screenshot tests for stable shared component states in
      light and dark themes
- [x] Pin viewport, color scheme, fonts, fixture data, and motion preferences for
      screenshot runs

Verification:

- `pnpm --filter @arclightning/ui check`
- `pnpm --filter @arclightning/ui test`
- `pnpm --filter @arclightning/ui test:browser`
- `pnpm --filter @arclightning/ui test:visual`
- `pnpm --filter @arclightning/ui build`
- Review light, dark, narrow, hover, focus, disabled, loading, empty, and error
  states
