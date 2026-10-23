+++
id = "arcl-ph-01M5NH1M603NDS40YKKA2R150W"
plan = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
title = "Milestone 6: Shared interface system"
status = "completed"
position = 6
+++

Create the Svelte UI package before building the graphical product surfaces.

Deliverables:

- a pnpm workspace containing `apps/desktop`, `apps/website`, and the shared UI
  package
- the existing root website moved to `apps/website` without changing its
  behavior
- semantic light and dark theme tokens using navy/sky blue with
  gold/amber/yellow accents
- accessible controls, navigation, record summaries, status treatments, and
  Markdown presentation
- Playwright-backed Vitest browser tests for shared Svelte behavior
- Playwright screenshot baselines and a development surface for reviewing
  component states

Exit criterion: the website and a desktop test shell consume the same packaged
components without importing application-specific adapters
