- Include rustdoc/tsdoc comments for exported & contextually important symbols
- In Rust code, never separate impls from their types and avoid `unsafe`
  - Order: constants, enums, structs, then free functions. pub before private/local
  - Use only 1 level of super (i.e. no `super::super::`)
  - Import shared crate items through `arcl_core::prelude::*` and
    `arcl_store::prelude::*` rather than long item lists
- Use the writing-docs skill and keep communication direct & succinct while retaining
  the user's voice. Describe the current product directly instead of framing it
  against earlier versions.
- Linting must clear even if warnings are unrelated.
- Disk is limited. After you address review changes, run `cargo clean` (or delete
  your own `CARGO_TARGET_DIR`) so build output doesn't fill it.

<!-- fabrial:begin -->
## Branches and pull requests

This repository uses fabrial. `fab init` wrote this section and the files it
names, `fab update` refreshes them, and `fab check` reports where they have
drifted.

- Never commit to `main`. Push a `claude/<topic>`, `codex/<topic>` or
  `storm/<task>` branch; the Branch PR workflow opens the pull request as a
  draft from stormlight-thunderbot.
- Commits on those branches may carry the agent's attribution trailers. The
  squash commit replaces them with the PR title, the PR body above its
  `<!-- fabrial:footer` line, `Merged from <branch>.` and the owner's
  `Co-authored-by` line from `.github/fabrial.toml`. Attribution lines and
  session links in the body are dropped, so nothing on `main` carries AI
  attribution. Editing the title or body changes the squash message without
  asking for a new approval.
- Before pushing, run the command `gates` names in `.github/fabrial.toml`.
- The owner's approval merges the PR. Agents never approve or merge.
- Keep the PR body to what changed and why. Put verification in a PR comment.
  Leave out attribution lines and session links.
- Agents post PR comments as stormlight-thunderbot through the Bot comment
  workflow (`.github/workflows/comment.yml`), never with their own GitHub
  access.
- The agent that opens a PR reviews it with the `storm` skill and ends with a
  `Review finished at <sha>` comment. That turns `storm/review` green for that
  head, marks the draft ready and turns on squash auto-merge. A PR that can
  merge already gets the squash message in a comment, for a manual merge.
  `/storm:ready <sha>` from the owner, a member or a collaborator accepts that
  head as it is, and `/storm:stop` halts auto-merge.

## Writing

Everything here is read by a person: docs, commit messages, PR titles and
bodies, comments, and replies. Keep it short and plain:

- Lead with the point, and say each thing once.
- Use short sentences, common words, and one term for one thing.
- Cut preambles, summaries of what was just said, and sign-offs.
- Name the property, the limit or the failure, not a judgment of it.
- Stay inside the length targets: a PR body around 150 words, a review comment
  200, any other comment 100.

`.claude/skills/writing-docs/` holds the full standard, and its
`references/tells.md` lists the patterns that make text read as generated.
Check a draft against both before committing or posting.
<!-- fabrial:end -->
