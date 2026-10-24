+++
id = "arcl-c-01M5QQ5M42AJ65TKHQQXDD6TJQ"
title = "Keep development skills out of npx skills add"
status = "captured"
created-at = "2026-10-24T19:45:57.890Z"
+++

`npx skills add stormlightlabs/arclightning --list` offers the fabrial development skills in `.claude/skills/` next to `arclightning`, so users must pass `--skill arclightning`. It also skips `review` with a YAML parse error on its unquoted `argument-hint: [pr-number|branch] ...`. Both files come from fabrial, so the fix belongs there: quote the hint, and consider `metadata.internal: true` on those skills.
