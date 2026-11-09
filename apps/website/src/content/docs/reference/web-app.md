---
title: Web app views and shortcuts
description: The Work, Task, Inbox and Planning views of arcl serve, the board moves, the command menu, every keyboard shortcut and the quick capture tokens.
---

`arcl serve` serves the web app. It has four views: Work for tasks, Task for
one task, Inbox for captures, and Planning for specs, plans and phases. Every
change runs the `arcl` command named here, with the same validation. A
refused change shows the CLI's message and error code, and a change whose
sync or export warned shows the warnings in a notice.

## Work

The Work view lists every task by phase, then by status, in `arcl tree`
order: phases by plan and position, tasks by position, each followed by its
subtasks. The app labels phases as milestones, and tasks outside any phase
fall under **No milestone**.

A glyph leads each row. For a pending or parked task, hover over it or focus
the row to read why the task is not ready. Filters narrow the list by status,
priority, phase and readiness: ready, blocked by an incomplete task, or
waiting for another reason, such as open subtasks. The page URL keeps the
filters. A task deleted while the list loads is left out; any other failed
read shows an error with a Retry button.

**New task** opens a form for the title, priority and description, runs
`arcl task create`, and opens the task.

### Board

**List** and **Board** beside the title, or `v`, switch the layout. The
browser remembers the choice, and the URL names the board as `?view=board`.
The board has a column per status: Pending, In progress, Parked, Completed and
Cancelled. Its cards show the task's ID, title, readiness glyph and priority,
link to the Task view, and follow the filters. A long column scrolls inside
itself, and a narrow window scrolls the board sideways.

| From                 | To          | Runs                 |
| -------------------- | ----------- | -------------------- |
| Pending              | In progress | `arcl task start`    |
| Pending, In progress | Parked      | `arcl task park`     |
| Parked               | Pending     | `arcl task unpark`   |
| Pending, In progress | Completed   | `arcl task complete` |

A move to Completed first asks for optional evidence. A drop on Cancelled is
refused with a message that names `arcl task cancel`, and any other drop the
table leaves out is refused with a message too. No command reopens a closed
task. While a move runs, no other card moves. A card moves once its command
succeeds. Each card's **Move** button, or `m` on the selected card, opens a
dialog with the moves it allows.

## Task

The Task view shows what `arcl show <task-id>` returns: why the task is not
ready, its direct blockers, the blockers it inherits from each ancestor, its
subtasks, the tasks it unblocks, the latest handoff and the evidence. Each
task it names links to that task's view. The description renders as
Markdown, and **View Markdown** opens the source in a read-only editor.

The properties panel lists status, readiness, priority, parent, spec, plan
and phase. `]` collapses and expands it, also from inside the read-only
source. `Escape` returns to the Work list with its filters and the task
selected, except while an open form holds unsaved input. Inside the source,
the first `Escape` moves focus out of the editor.

| Status      | Buttons                  |
| ----------- | ------------------------ |
| pending     | Start, Park              |
| in progress | Complete, Hand off, Park |
| parked      | Unpark                   |

Complete asks for optional Markdown evidence and Hand off for a Markdown
note, which parks the task. **Edit** opens the title, priority and
description in a writable editor; Save sends only the fields that changed.
Each button runs the `arcl task` command of the same name. After a change the
view reloads the task, and after a completion it lists the tasks the
completion made ready.

**Edit file** opens the task's workspace file, with the TOML front matter
between the `+++` lines highlighted as TOML and the body as Markdown. **Save
file** sends it to `POST /api/records/<id>/edit`, which runs the checks of
`arcl workspace import`. The file cannot change the task's status, handoff
note or evidence. A refused file keeps the edit open. When the file changed
since it was opened, **Reload the file** loads the new version in place of
the edit.

## Inbox

The Inbox lists the captures not yet promoted or discarded, oldest first, and
shows the selected one's body, with a link to the task it came from.

**Promote to task**, **Promote to spec** and **Make a note** open a form with
the capture's title and run `arcl capture promote`; the new record gets the
capture's body. The task form sets the priority, phase and parent, and reads
`!priority` and `^REF` from the capture's title. A promotion takes no
blockers, so the form names each `after:REF` for you to add with
`arcl dependency add`. The spec form takes optional acceptance criteria.
**Discard** asks first, then runs `arcl capture discard`.

After either, the next capture is selected, and a notice links to the new task
or to Planning. While a form holds edits, or a promotion or discard is
running, `j`, `k` and clicking another capture keep the selection; with
edits, they return focus to the form. A capture already promoted or discarded
elsewhere, such as from the CLI, reloads the list with a notice.

## Planning

Planning lists each spec with its plans, and each plan with its phases in
`arcl tree` order. A phase row shows whether the phase is open or done, how
many of its tasks are done, and how many are ready or blocked; subtasks
count. `Enter` opens the selected phase's tasks and subtasks below it, or
collapses and expands a spec or plan. Closed specs and plans start collapsed.
A task in the tree opens its Task view.

**New spec** runs `arcl spec create`, **New plan** on a spec runs
`arcl plan create`, and **Apply plan** on a plan takes a TOML document and
runs `arcl plan apply`. The view reloads in place after each change.

## Command menu

`⌘K`, or `Ctrl+K` off macOS, opens the command menu, as does **Search** in the
sidebar. The menu lists the actions on the selection first: the row selected
in the Work list, the Task view's task, the Inbox's capture, or the selected
spec or plan in Planning. The open view's commands, Quick capture and Go to
follow, and once you type, the tasks whose titles match.

Each command shows its shortcut. Typing filters the list, the arrow keys move,
`Enter` runs the command and `Escape` closes the menu. `Backspace` in an empty
search drops the selected task from the menu. A command runs the same change
as its button. From the Work list, Complete and Hand off open the Task view
with their form.

## Keyboard shortcuts

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
text with `arcl capture create`, keeping the first line as typed. `⌘Enter`, or
`Ctrl+Enter` off macOS, files it as a task with `arcl task create` and opens
it. Tokens in the first line then set the task's fields:

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
text: `\!high` puts `!high` in the title.

## Theme and layout

The theme follows the system's light or dark setting: light grey and blue, or
black and blue. Text in both themes meets WCAG AA contrast. Animations and
transitions stop when the system asks for reduced motion. In a window
narrower than 48rem the sidebar becomes a bar at the top, and **Menu** shows
Search, Capture and the views; choosing a view closes it.

## Server

The server binds `127.0.0.1` and refuses requests without the token or with
a `Host` that does not name it. A change also needs an `Origin` that names
the server. [REST endpoints](/reference/rest/#security) lists every check.
