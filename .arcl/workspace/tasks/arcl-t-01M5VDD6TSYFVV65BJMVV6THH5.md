+++
id = "arcl-t-01M5VDD6TSYFVV65BJMVV6THH5"
phase = "arcl-ph-01M5NE5P5RE8Y5Y5Y84T36FMEE"
title = "Make Edit and Edit file distinguishable"
status = "completed"
priority = "normal"
position = 11
evidence = "Delivered by #PR: `edit` (i-ri-pencil-line) and `editFile` (i-ri-file-edit-line) join `ICONS`. The Task view's Edit and Edit file buttons show them, aria-hidden, before their labels, and `task:edit` and `task:edit-file` use the same icons in the command menu. A DOM test in TaskDetail.dom.test.ts checks each button's icon, its unchanged name and its command's icon. The visual baselines are gitignored, so the Task view was checked by eye in a Playwright screenshot instead."
+++

Tell Edit and Edit file apart in the Task view. The action row reads
"Start / Park / Edit / Edit file", two ghost buttons that differ only by one
word, and the command menu gives both the `markdown` icon.

Edit opens the field editor and Edit file opens the record's raw file. Give
Edit a pencil icon (`i-ri-pencil-line`) and Edit file a pencil-on-file icon
(`i-ri-file-edit-line`), and use the same icons for `task:edit` and
`task:edit-file` in the command menu.

Scope:

- `edit` and `editFile` entries in `ICONS`, and `pnpm icons:generate`
- the icons on both buttons and both commands

Acceptance criteria:

- [x] Edit shows a pencil icon and Edit file a pencil-on-file icon, each
      beside its text label
- [x] The buttons keep their accessible names, "Edit" and "Edit file"
- [x] The command menu shows the same icon for each command as its button

Verification:

- a DOM test that each button renders its icon and keeps its name
- `pnpm test:visual:update` for the Task view baseline, reviewed by eye
- `pnpm check`, `pnpm test`, `just check`
