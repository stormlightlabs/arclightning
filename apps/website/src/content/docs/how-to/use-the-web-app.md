---
title: Use the web app
description: Start arcl serve, triage the inbox, move tasks on the board, hand off and complete work, and edit a task's file in the browser.
---

The web app runs the same operations as the CLI, with the same validation, so
you can use both side by side. [Web app views and
shortcuts](/reference/web-app/) describes every view and key.

## Start the server

```sh
arcl serve
```

```text
Serving Arc Lightning at http://127.0.0.1:43221/?token=…
Press Ctrl-C to stop.
```

Open the printed URL in a browser. Only the tab that opens it can use the
server, since the URL carries a token for the first page load. `--port <PORT>`
pins the port. Other `arcl` commands keep working while it serves. Press
Ctrl-C to stop it.

If `serve` shows a page saying the web app was not built, your `arcl` was
compiled without it. [Install arcl](/how-to/install/#build-from-source) shows
how to build one with it.

## See changes made outside the browser

The web app reads the database directly and does not sync the workspace
first. After editing workspace files or pulling, run any `arcl` command, such
as `arcl workspace sync`, and reload the page.

## Triage the inbox

Press `g` then `i` to open the Inbox. It lists the open captures, oldest
first. For each one, pick an action:

- **Promote to task** (`1`) sets the priority, phase and parent. `!priority`
  and `^REF` in the capture's title fill in the form.
- **Promote to spec** (`2`) takes optional acceptance criteria.
- **Make a note** (`3`).
- **Discard** (`Backspace`) asks first.

A promotion takes no blockers. The task form names each `after:REF` in the
title so you can add it with `arcl dependency add`.

## Work a task on the board

Press `g` then `w` for the Work view, and `v` to switch from the list to the
board. Drag a card to another column to run the command for that move. A
move from Pending to In progress starts the task. A move to Completed asks
for optional evidence first. Each card's **Move** button, or `m` on the selected
card, offers the same moves from the keyboard.

The board has no cancel action. Cancel a task with `arcl task cancel`.

## Hand off or complete from the Task view

Press `Enter` on a task to open it. The buttons under the title follow the
task's status:

| Status      | Buttons                  |
| ----------- | ------------------------ |
| pending     | Start, Park              |
| in progress | Complete, Hand off, Park |
| parked      | Unpark                   |

**Hand off** asks for a Markdown note and parks the task. **Complete** asks
for optional Markdown evidence, and afterwards the view lists the tasks the
completion made ready. A refused change shows the CLI's message next to its
button.

## Edit a task's file

**Edit file** in the Task view opens the task's whole workspace file. Change
the front matter or the body, then **Save file**. The file passes the same
checks as `arcl workspace import`. It cannot change the status, handoff or
evidence; use the buttons for those. When the file changed since you opened
it, **Reload the file** loads the new version.

## Capture from anywhere

Press `q`. Type a title on the first line, and press `Shift+Enter` to start a
body. `Enter` files a capture. `⌘Enter`, or `Ctrl+Enter` off macOS, files a
task instead and reads `!high`, `^REF` and `after:REF` in the first line.
