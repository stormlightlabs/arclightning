+++
id = "arcl-c-01M5QTPAPZRJ43T18DAYX81BF9"
title = "Shrink the web app's inline fonts"
status = "captured"
created-at = "2026-10-24T20:47:31.039Z"
+++

The web app builds with SvelteKit's inline bundle strategy so `index.html` is the only file outside the API, since the browser sends no token header for assets. Fonts are inlined as data URIs with every subset of Google Sans, Google Sans Code, IBM Plex Sans and Inter, so `index.html` is about 2 MB. Latin-only subsets, or loading fonts with fetch and the FontFace API, would cut it. Found in T18.1.
