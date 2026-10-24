+++
id = "arcl-n-01M5QS2FRJ5B3AFHD80ARSQ0K6"
title = "Decision: Web app stack and look"

[[links]]
kind = "phase"
id = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"

[[links]]
kind = "plan"
id = "arcl-pl-01M5NE5P0CWMJNP2A7B0ZQW6XC"
+++

The web app is Svelte and shares `packages/ui` with the desktop app. Styling is UnoCSS, with the icons preset serving Phosphor, Bootstrap and Remix icons from Iconify. Headings use Google Sans and body text uses Inter. The light theme is light grey and blue; the dark theme is black and blue. The layout follows Linear's patterns: a sidebar, dense grouped lists, a detail view with a properties panel, and a command menu. T18 lands as seven stacked PRs, each based on the one before.

Decided 2026-10-24.
