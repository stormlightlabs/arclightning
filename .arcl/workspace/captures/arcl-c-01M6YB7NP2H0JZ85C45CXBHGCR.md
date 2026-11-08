+++
id = "arcl-c-01M6YB7NP2H0JZ85C45CXBHGCR"
title = "Serve a PNG social card for the website"
status = "captured"
created-at = "2026-11-08T19:47:44.962Z"
+++

The landing page and docs pages set `og:image` to /social-card.svg. Facebook, LinkedIn and X do not render SVG link previews, so shared links show no image. Render the card to a 1200x630 PNG at build time or commit one, and point `og:image` at it. Found while building the website shell.
