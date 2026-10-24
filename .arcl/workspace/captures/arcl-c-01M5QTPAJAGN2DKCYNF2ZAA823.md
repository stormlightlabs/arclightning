+++
id = "arcl-c-01M5QTPAJAGN2DKCYNF2ZAA823"
title = "Reload in the web app loses access"
status = "captured"
created-at = "2026-10-24T20:47:30.890Z"
+++

`arcl serve` accepts the query token once, for the first page load, and the browser sends no header on a reload or a new tab, so both get 401 after the first load. The app keeps the token in session storage; a reload path could use it, such as a 401 page for document requests that refetches with the header, or a new URL printed on demand. Found in T18.1.
