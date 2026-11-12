+++
id = "arcl-r-01M75WTEZKDT5QBWJ4QYFQXBVB"
title = "v0.4.1"
status = "completed"

[[members]]
kind = "task"
id = "arcl-t-01M5NE5P8KF13P2YZ4B8W2B7MS"

[[members]]
kind = "task"
id = "arcl-t-01M5S4NCTDZ15M40786J4V7AYB"

[[members]]
kind = "task"
id = "arcl-t-01M5T1MS640ZDRP6XTZD3MEMK6"

[[members]]
kind = "task"
id = "arcl-t-01M5T1MS8H4SEVN8HXF7BWNWCT"

[[members]]
kind = "task"
id = "arcl-t-01M5VDD6RKGADMNXF7KH4H0J83"

[[members]]
kind = "task"
id = "arcl-t-01M5VDD6TSYFVV65BJMVV6THH5"

[[members]]
kind = "task"
id = "arcl-t-01M5VDD6X6EP6WH4CMDJSGQ87N"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFB4XC9E438GSNCV444XZ"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFB6X9AQQ43BBZVYJWWC8"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFB8NYG5KXXNBF3TSH9ZQ"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBB350J9B1FRSK8QPKMQ"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBDAPHHXFZK82G8BT0G4"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBF5V803RBTEDG12QZJP"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBGVPHMEZY9GGJFS4DZR"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBJMNQ3R0C6ACT7ZH08Q"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBMHGP74F6CJSSG7FXP7"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBPD4ZGFC56Q2WCSQHHN"

[[members]]
kind = "task"
id = "arcl-t-01M5YRFBR86YBZRSTWJRQBE0KB"

[[members]]
kind = "task"
id = "arcl-t-01M60C3V1V1MM3YHDSWXSY6X3A"

[[members]]
kind = "task"
id = "arcl-t-01M61HW4VBTNF4G8PEJ6G996Z4"

[[members]]
kind = "task"
id = "arcl-t-01M61HW4YQ3YFTES2BZ8X27MXW"

[[members]]
kind = "task"
id = "arcl-t-01M61HW51Z6X4YHYAB35AYZDQ8"

[[members]]
kind = "task"
id = "arcl-t-01M6YAFBCE29665W3ZHAVA5F18"

[[members]]
kind = "task"
id = "arcl-t-01M6YVG52P5TSEFQ415C9Q668B"

[[members]]
kind = "task"
id = "arcl-t-01M747RKR3EKZKVA46M448ES5B"

[[members]]
kind = "task"
id = "arcl-t-01M747RKVZ7AS7XF1DAWSREFJQ"

[[members]]
kind = "task"
id = "arcl-t-01M747RKZF3YMX5W36BVN3WJVW"

[[members]]
kind = "task"
id = "arcl-t-01M747RM2Z77GBTM4PQ8ZDZGDW"

[[members]]
kind = "task"
id = "arcl-t-01M747RM6BBYQBEMG2DWPFEBBJ"

[[members]]
kind = "task"
id = "arcl-t-01M747RM9W8RWDG9N8RH30QAYQ"

[[members]]
kind = "task"
id = "arcl-t-01M75WSPEX0F7MT8E7GPT3FTNE"
+++

Ships the agent workflow milestone. `arcl setup` installs the skill, adds the arcl lines to the agent's instruction file and, for Claude Code, Codex and Pi, adds a session-start hook that runs `arcl prime`. `arcl prime` prints the agent loop, open work with its handoffs and the first ready tasks. `arcl task start --claim` refuses a task someone else holds, and `arcl next --start` picks and starts the next ready task in one step. Tasks take handoffs, notes with a kind (decision, observation, blocker or result) and gates on a pull request, a CI run or a date. A capture records the task it came from. `--blocked-by` takes one ID per flag, repeatable or comma-separated, and in a fresh worktree the missing-database error says that `arcl init` imports the workspace. `arcl roadmap` prints specs, plans, phases and releases as one Markdown page, and `arcl compact` archives old completed tasks into one summary note per phase.

The MCP server's `list` tool filters by priority, release, spec, plan, phase and parent, and a new `prime` tool returns the primer. `arcl query` runs read-only GraphQL, `arcl serve` answers GraphQL at `/graphql` and REST reads under `/api`, and `arcl serve --api` serves those without the web app. For hosting, `arcl serve` takes `--bind`, `--token-file` or `ARCL_SERVE_TOKEN`, `--allow-host` and `--project`, answers `GET /healthz` without a token and reports the version at `GET /api/version`.

arcl reads and writes workspace formats up to 6, and the database moves to schema 9. Claiming, noting, gating or completing a task, or `capture create --from`, writes a format arcl 0.3.0 cannot read, so upgrade every clone and CI job first. arcl refuses a database or workspace written by a newer version, with an error that names both versions.

The website is rebuilt with the full documentation, and any branch except `main` can be deployed as a Cloudflare Pages preview from the Actions tab. The web app copies record IDs, marks Edit and Edit file with icons and shows no readiness on closed tasks. The `v0.4.0` tag has no release: its Apple Silicon build, queued for a macos-14 runner, never got one, so v0.4.1 ships the same work and builds that target on macos-15. "Prepare and tag v0.4.1" covers the release work itself. Tagged `v0.4.1` on 773432c (#101). Install with `curl --proto '=https' --tlsv1.2 -LsSf https://github.com/stormlightlabs/arclightning/releases/download/v0.4.1/arcl-cli-installer.sh | sh` or `brew install stormlightlabs/tap/arcl`.
