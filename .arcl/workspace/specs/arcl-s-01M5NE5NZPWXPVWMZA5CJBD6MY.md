+++
id = "arcl-s-01M5NE5NZPWXPVWMZA5CJBD6MY"
title = "Arc Lightning first release and desktop app"
status = "open"
acceptance-criteria = """
First release:

- a non-Git directory can host a complete Arc Lightning project
- schema changes preserve descriptions, relationships, lifecycle state,
  handoffs, evidence, and stable identifiers when their type is still valid
- captures, specs, plans, optional phases, tasks, notes, and releases are
  persistent domain objects
- releases use a general membership relationship for specs, plans, tasks, and
  notes
- specs and plans own their Markdown content instead of pointing at documents
  that Arc Lightning cannot edit
- every supported operation uses the same Rust application layer from the CLI,
  web, and MCP adapters
- flexible task placement preserves deterministic readiness and dependency
  rules
- automations can read why work is blocked and what a completion unblocked as
  typed data
- repository-native mode round-trips the complete model and refuses to
  overwrite divergent database and file changes
- the local web app provides Inbox, Work, Planning, and Task views
- the web app and website consume one shared UI package for design tokens and
  reusable Svelte components
- the documentation follows the Stormlight house style described below
- human output is concise and machine output is versioned and stable
- focused tests cover domain rules, storage migrations, adapters, UI behavior,
  and repository synchronization
- Playwright screenshot tests cover stable web app, website, and shared UI
  states in light and dark themes

Desktop app, after the first release:

- the Tauri app provides the same four views through the same Rust application
  layer
- the Tauri app consumes the shared UI package
- Playwright screenshot tests cover the desktop views in light and dark themes"""
+++

`SPEC.md` defines the product. This spec tracks its success criteria.
