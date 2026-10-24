+++
id = "arcl-c-01M5QZXFFNK5VVBCZX4EHW488R"
title = "Let a reload of the web app keep its session"
status = "captured"
created-at = "2026-10-24T22:18:48.181Z"
+++

`arcl serve` needs the token header on page loads after the first, and a browser sends none on reload, so reloading any view, including a Work URL with filters, returns 401. Found in T18.2.
