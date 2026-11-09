---
title: Releases and roadmap
description: Group specs, plans, tasks and notes into named releases, and publish their progress as a Markdown roadmap.
---

A release is a named set of planned deliverables, such as `v0.1.0`. Specs,
plans, tasks and notes join a release as members. A record can be in more
than one release, and membership never includes a record's descendants
implicitly. Captures join only after they are promoted, and phases belong to
their plan.

## Create a release

```sh
arcl release create "v0.1.0" --body "First public release."
```

```text
Created release `arcl-r-01M6Z4XMB1F7CHMPEHNHYSVY2C`: v0.1.0
```

```sh
arcl release list
arcl release show SVY2C
arcl release update SVY2C --title "v0.1.0-beta" --body-file notes.md
```

## Add members

```sh
arcl release member add SVY2C --kind spec --record-id FVMC
arcl release member list SVY2C
arcl release member remove SVY2C --kind spec --record-id FVMC
```

```text
spec	arcl-r-01M6Z4XMB1F7CHMPEHNHYSVY2C	arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC
```

`--kind` is `spec`, `plan`, `task` or `note`. `member list` and
`arcl list --release <id>` both print the explicit members only, without the
records under them.

## Close a release

```sh
arcl release complete SVY2C
arcl release cancel SVY2C
```

A release with open records below its members refuses to close:

```text
error: invalid project or record: stored release is invalid: cannot complete release `arcl-r-01M6Z4XMB1F7CHMPEHNHYSVY2C` while it has non-terminal descendants; pass --allow-open-children to override
```

`--allow-open-children` closes the release alone. Release membership does not
affect readiness, so closing a release holds no task back.

## Publish a roadmap

`arcl roadmap` prints every spec with its plans and their phases, then the
releases, as one Markdown page for a README or a docs site:

```sh
arcl roadmap --output docs/roadmap.md
```

```markdown
# Roadmap

## Add keyboard shortcuts

`arcl-s-01M6Z4WFTDTZJMQ5EDE6GSFVMC` · in progress · 1/4 tasks

### Keyboard plan

`arcl-pl-01M6Z4WFW1XXC6XZ81Y9F6Z234` · in progress · 1/4 tasks

| Phase | Status | Tasks |
| --- | --- | --- |
| Key handling | in progress | 1/2 |
| Documentation | planned | 0/2 |
| Polish | planned | no tasks |

## Releases

| Release | Status | Tasks |
| --- | --- | --- |
| v0.1.0 | in progress | 1/4 |
```

Counts are completed tasks over all tasks at any depth, leaving out cancelled
ones. A release counts its member tasks and the tasks under its member specs
and plans, each once. A record is `planned` until a task below it starts,
then `in progress`, and `done` or `cancelled` once closed.

The page carries no timestamp, so the same records always give the same
bytes, and a CI job can fail when the committed roadmap is out of date:

```sh
arcl roadmap --output docs/roadmap.md
git diff --exit-code docs/roadmap.md
```

`--json` returns the page's data instead. The
[manual](/reference/manual/#roadmap) covers ordering, escaping and how
`--output` treats symbolic links.
