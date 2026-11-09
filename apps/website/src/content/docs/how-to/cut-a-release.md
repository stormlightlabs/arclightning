---
title: Cut a release and render the roadmap
description: Group specs, plans, tasks and notes into a named release, close it when its work is done, and publish progress as a Markdown roadmap.
---

## Create a release

```sh
arcl release create "v0.1.0" --body "First public release."
```

```text
Created release `arcl-r-01M70MJK5FNMS7CY4D1PG1G9R7`: v0.1.0
```

```sh
arcl release list
arcl release show G9R7
arcl release update G9R7 --title "v0.1.0-beta" --body-file notes.md
```

<div id="add-members"></div>

## Add what ships in it

```sh
arcl release member add G9R7 --kind spec --record-id JKHQ
arcl release member list G9R7
```

```text
spec	arcl-r-01M70MJK5FNMS7CY4D1PG1G9R7	arcl-s-01M70M8AY4K0WT6RN8C4H4JKHQ
```

To take a record out of the release:

```sh
arcl release member remove G9R7 --kind spec --record-id JKHQ
```

`--kind` is `spec`, `plan`, `task` or `note`. Captures join only after they
are promoted, and phases belong to their plan. A record can be in several
releases.

Membership is explicit. `member list` and `arcl list --release <id>` print
the members you added, not the records under them.

<div id="close-a-release"></div>

## Close the release

```sh
arcl release complete G9R7
arcl release cancel G9R7
```

A release with open records below its members refuses to close:

```text
error: invalid project or record: stored release is invalid: cannot complete release `arcl-r-01M70MJK5FNMS7CY4D1PG1G9R7` while it has non-terminal descendants; pass --allow-open-children to override
```

Finish that work first, or pass `--allow-open-children` to close the release
alone. Release membership does not affect readiness, so closing a release
holds no task back.

<div id="publish-a-roadmap"></div>

## Render the roadmap

`arcl roadmap` prints every spec with its plans and phases, then the
releases, as one Markdown page. `--output` writes it to a file:

```sh
arcl roadmap --output docs/roadmap.md
```

```markdown
# Roadmap

## Add keyboard shortcuts

`arcl-s-01M70M8AY4K0WT6RN8C4H4JKHQ` · in progress · 3/5 tasks

### Keyboard plan

`arcl-pl-01M70M8FPNJTRS3HK1EZ7RDQ8G` · in progress · 3/5 tasks

| Phase | Status | Tasks |
| --- | --- | --- |
| Key handling | in progress | 3/4 |
| Documentation | planned | 0/1 |
| Polish | planned | no tasks |

## Releases

| Release | Status | Tasks |
| --- | --- | --- |
| v0.1.0 | in progress | 3/5 |
```

Counts are completed tasks over all tasks at any depth, leaving out cancelled
ones. A release counts its member tasks and the tasks under its member specs
and plans, each once. A record is `planned` until a task below it starts,
then `in progress`, and `done` or `cancelled` once closed.

## Keep the committed roadmap current

The page carries no timestamp, so the same records give the same bytes. Commit
the file, and a CI step can fail when it falls behind:

```sh
arcl roadmap --output docs/roadmap.md
git diff --exit-code docs/roadmap.md
```

`--json` prints the page's data instead. The
[manual](/reference/manual/#roadmap) covers ordering, escaping and how
`--output` treats symbolic links.
