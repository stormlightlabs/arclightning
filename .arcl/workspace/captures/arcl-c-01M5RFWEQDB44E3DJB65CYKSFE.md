+++
id = "arcl-c-01M5RFWEQDB44E3DJB65CYKSFE"
title = "Align the website with the grey and blue tokens"
status = "captured"
created-at = "2026-10-25T02:57:51.853Z"
+++

T18.7 moved packages/ui tokens.css to the web app's light grey and blue and black and blue themes. apps/website/src/styles/theme.css still hardcodes the old navy header and code colors (--site-header, --code-surface, --shadow) and uses --arcl-warning for eyebrows and link hover, which read as the old gold accent.
