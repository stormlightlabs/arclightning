+++
id = "arcl-c-01M5R102JK05522QKZA5C09NS0"
title = "Keep the web app bundle under Vite's size warning"
status = "captured"
created-at = "2026-10-24T22:37:41.843Z"
+++

CodeMirror 6 brings the inlined app script to about 650 kB, over Vite's 500 kB chunk warning. The `inline` bundle strategy rules out loading the editor on demand as a separate file. Found in T18.3.
