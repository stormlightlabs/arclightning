+++
id = "arcl-c-01M6Z8K9MESSBPV5TEK5EF10EX"
title = "Stop telling users to commit records restored after a branch switch"
status = "captured"
created-at = "2026-11-09T04:20:54.542Z"
+++

After switching to a branch that lacks records synced on another branch, `arcl workspace resolve --keep database <path>` or `arcl workspace export` writes those files back as untracked. `arcl workspace status` then says "Sync has nothing to carry; commit these files in Git." Committing them copies the other branch's records onto this one, and while they stay untracked Git refuses to switch back. Status and the sync error could name the branch case and say not to commit. Found while revising the workspace guide on PR #97.
