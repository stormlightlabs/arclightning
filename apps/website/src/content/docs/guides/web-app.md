---
title: Web app
description: Run the local web app with arcl serve to triage the inbox, work tasks on a list or board, and browse specs, plans and phases.
---

`arcl serve` runs Arc Lightning in the browser. The web app has four views:
Inbox for captures, Work for tasks, Planning for specs, plans and phases, and
Task for one task. Every change runs the same operation as the matching
`arcl` command, with the same validation, so the CLI and the browser can be
used side by side.

## Start the server

```sh
arcl serve
```

```text
Serving Arc Lightning at http://127.0.0.1:43221/?token=…
Press Ctrl-C to stop.
```

The server binds `127.0.0.1` on an ephemeral port; `--port <PORT>` pins one.
Open the printed URL. It carries a token for the first page load, which the
app moves out of the URL and sends in a header on every later request. Each
run makes a new token. The server runs until Ctrl-C, and other `arcl`
commands keep working while it serves.

Only the browser tab that opened the URL can use the server. It refuses a
request without the token or with a `Host` header that does not name it, and
a change also needs an `Origin` that names it, which stops other local pages,
including those on other `127.0.0.1` ports, from driving it. The
[API guide](/guides/api/#security-and-limits) lists every check.

The web app reads the database directly and does not sync the workspace
first. A change syncs as a command does. After editing workspace files or
pulling, run any `arcl` command, such as `arcl workspace sync`, and reload.

## Work

The Work view lists every task by phase, then by status, in the order
`arcl tree` gives: phases by plan and position, and tasks by position, each
followed by its subtasks. The app labels phases as milestones, and tasks outside
any phase fall under **No milestone**. A glyph leads each row; for a pending or
parked task, hover over it or focus the row to read why the task is not ready.
`j` and `k` move the selection, and `Enter` opens the task. Filters narrow the
list by status, priority, phase and readiness: ready, blocked by an incomplete
task, or waiting for another reason, such as open subtasks. The page URL keeps
the filters. A task deleted while the list loads is left out; any other failed
read shows an error with a Retry button. **New task** opens a form for the
title, priority and description, creates the task with `arcl task create`, and
opens it.

**List** and **Board** beside the title, or `v`, switch the layout. The
choice is remembered in this browser, and the URL names the board as
`?view=board`. The board has a column for each status: Pending, In progress,
Parked, Completed and Cancelled. Its cards show the task's ID, title,
readiness glyph and priority, link to the Task view, and follow the filters.
A long column scrolls inside itself, and a narrow window scrolls the board
sideways. Dragging a card to another column runs the command that makes that
move:

| From                 | To          | Runs                 |
| -------------------- | ----------- | -------------------- |
| Pending              | In progress | `arcl task start`    |
| Pending, In progress | Parked      | `arcl task park`     |
| Parked               | Pending     | `arcl task unpark`   |
| Pending, In progress | Completed   | `arcl task complete` |

A move to Completed first asks for optional evidence. The web app has no
cancel action, so a drop on Cancelled is refused with a message that names
`arcl task cancel`. No command reopens a closed task. Any other drop the
table leaves out is refused with a message too, and the card stays in its
column. While a move runs,
no other card moves. A card moves once its command succeeds; a
refused change shows the CLI's message. Each card's **Move** button, or `m`
on the selected card, opens a dialog with the moves it allows, so the
keyboard reaches every move.

## Task

The Task view shows what `arcl show <task-id>` returns: why the task is not
ready, its direct blockers, the blockers it inherits from each ancestor, its
subtasks, the tasks it unblocks, the latest handoff and the evidence. Each
task it names links to that task's view. The description renders as Markdown,
and **View Markdown** opens the source in a read-only editor. The properties
panel lists status, readiness, priority, parent, spec, plan and phase; `]`
collapses and expands it, also from inside the read-only source. `Escape`
returns to the Work list with its filters and the task selected, except while
an open form holds unsaved input; inside the source, the first `Escape` moves
focus out of the editor. A failed read shows an error with a Retry button.

Buttons under the title change the task. A pending task offers Start and
Park, an in-progress task Complete, Hand off and Park, and a parked task
Unpark. Complete asks for optional Markdown evidence and Hand off for a
Markdown note, which parks the task. **Edit** opens the title, priority and
description in a writable editor; Save sends only the fields that changed.
Each change runs the `arcl task` command of the same name, so the CLI's
validation applies. A refused change shows the CLI's message and error code
next to its button. A saved change whose sync or export warned shows the
warnings in a notice. After a change the view reloads the task, and after a
completion it lists the tasks the completion made ready.

**Edit file** opens the task's whole workspace file in an editor, with the
TOML front matter between the `+++` lines highlighted as TOML and the body as
Markdown. **Save file** sends it to `POST /api/records/<id>/edit`, so the
file passes the checks `arcl workspace import` runs. The file cannot change
the task's status, handoff note or evidence; the buttons and the board do
that. A refused file shows the CLI's message and keeps the edit open; when
the file changed since it was opened, **Reload the file** loads the new
version in place of the edit.

## Inbox

The Inbox lists the captures not yet promoted or discarded, oldest first, and
shows the selected one's body, with a link to the task it came from. `j` and
`k` move the selection. **Promote to task**, **Promote to spec** and **Make a
note** open a form with the capture's title, which runs `arcl capture
promote`; the new record gets the capture's body. The task form sets the
priority, phase and parent, and reads `!priority` and `^REF` from the
capture's title. A promotion takes no blockers, so the form names each
`after:REF` for you to add with `arcl dependency add`. The spec form takes
optional acceptance criteria.
**Discard** asks first, then runs `arcl capture discard`. After either, the
next capture is selected, and a notice links to the new task or to Planning.
While a form holds edits, or a promotion or discard is running, `j`, `k` and
clicking another capture keep the selection; with edits, they return focus to
the form. A capture already promoted or discarded elsewhere, such as from the
CLI, reloads the list with a notice.

## Planning

Planning lists each spec with its plans, and each plan with its phases in
`arcl tree` order. A phase row shows whether the phase is open or done, how
many of its tasks are done, and how many are ready or blocked; subtasks
count. `j` and `k` move between specs, plans and phases, and `Enter` opens
the selected phase's tasks and subtasks below it, or collapses and
expands a spec or plan. Closed specs and plans start collapsed. A task in the
tree opens its Task view. **New spec** runs `arcl spec create`, **New plan**
on a spec runs `arcl plan create`, and **Apply plan** on a plan takes a TOML
document and runs `arcl plan apply`. Applying again with the same keys updates
the plan's phases and tasks. The view reloads in place after each change.

## Command menu and shortcuts

`⌘K`, or `Ctrl+K` off macOS, opens the command menu; so does **Search** in the
sidebar. The menu lists the actions on the selection first: the row
selected in the Work list, the Task view's task, the Inbox's capture, or the
selected spec or plan in Planning. The open view's
commands, Quick capture and Go to follow, and once you type, the tasks whose
titles match. Each command shows its shortcut. Typing filters the list, the
arrow keys move, `Enter` runs the command and `Escape` closes the menu.
`Backspace` in an empty search drops the selected task from the menu. A
command runs the same change as its button; from the Work list, Complete and
Hand off open the Task view with their form.

| Keys          | Command                                                       |
| ------------- | ------------------------------------------------------------- |
| `⌘K` `Ctrl+K` | Open the command menu                                         |
| `q`           | Quick capture                                                 |
| `g` `i`       | Go to Inbox                                                   |
| `g` `w`       | Go to Work                                                    |
| `g` `p`       | Go to Planning                                                |
| `n`           | New task, in the Work list                                    |
| `s`           | Start the selected task                                       |
| `c`           | Complete the selected task                                    |
| `h`           | Hand off the selected task                                    |
| `p`           | Park the selected task                                        |
| `u`           | Unpark the selected task                                      |
| `e`           | Edit the task, in the Task view                               |
| `v`           | Switch between list and board, in Work                        |
| `m`           | Move the selected card, on the board                          |
| `j` `k`       | Select the next or previous task, in Work                     |
| `Enter`       | Open the selected task, in Work                               |
| `]`           | Collapse or expand properties, in a task                      |
| `Escape`      | Return to Work, in a task                                     |
| `j` `k`       | Select the next or previous row, in Inbox and Planning        |
| `1`           | Promote the capture to a task, in Inbox                       |
| `2`           | Promote the capture to a spec, in Inbox                       |
| `3`           | Make the capture a note, in Inbox                             |
| `Backspace`   | Discard the capture, in Inbox                                 |
| `n`           | New spec, in Planning                                         |
| `a`           | New plan on a spec, or Apply plan on a plan, in Planning      |
| `Enter`       | Open or close the selected row, in Planning                   |
| `Escape`      | Close the open form if it has no edits, in Inbox and Planning |

Shortcuts do nothing while you type in a field, the editor or a dialog, or
hold `Ctrl`, `⌘` or `Alt`. Press `g` and its second key within a second.

## Quick capture

`q`, or **Capture** in the sidebar, opens quick capture from any view. The
first line is the title, and `Shift+Enter` starts the body. `Enter` files the
text with `arcl capture create`. `⌘Enter`, or `Ctrl+Enter` off macOS, files it
as a task with `arcl task create` and opens it; tokens in the first line then
set the task's fields:

| Token       | Sets                                                       |
| ----------- | ---------------------------------------------------------- |
| `!high`     | The priority, from `!critical`, `!high`, `!normal`, `!low` |
| `^REF`      | The parent task                                            |
| `after:REF` | A blocker; repeat it for more than one                     |

`REF` is a task ID, such as `arcl-t-01M5…`, or a task number, such as `T18.4`
for `T18.4: Add task mutations`. A reference that names no task or several
stops the task with an error. With two `^REF`, only the last is looked up.
Only a whole word is a token, so `!later`, `^C` and `after:lunch` stay text,
and the body is never read for tokens. A leading backslash keeps a token as
text: `\!high` puts `!high` in the title. A capture holds only a title and a
body, so `Enter` keeps the first line as typed, tokens and backslashes
included.

A change from the menu or quick capture whose sync warned shows the warnings in
a notice, as the Task view does.

## Theme and layout

The theme follows the system's light or dark setting: light grey and blue, or
black and blue. Text in both themes meets WCAG AA contrast. Animations and
transitions stop when the system asks for reduced motion. In a window narrower
than 48rem the sidebar becomes a bar at the top, and **Menu** shows Search,
Capture and the views; choosing a view closes it.

## Builds without the web app

`arcl` embeds the web app from `apps/web/build` when it compiles. Release
binaries include it. That build is not committed, so `cargo install --git`
produces an `arcl` whose `serve` shows a page saying the web app was not
built. To compile one with the web app, run `pnpm install` and
`pnpm --filter @arclightning/web build` in a checkout, then
`cargo install --path crates/arcl-cli`.
