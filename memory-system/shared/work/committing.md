# Committing

**Everything gets committed.** JJ said so on 2026 08 29. An uncommitted change is one bad
`checkout` away from being lost, and the next session cannot see why it is there.

Nothing is ever left unstaged as a way of keeping it out of a commit. That is not tidiness, it
is a change sitting unprotected.

## Unrelated work gets its own commit

If the working tree already holds somebody else's edit, do not sweep it into yours and do not
leave it behind. Commit it separately, say in the message that it was not yours and why it is
being committed anyway, and push it with the rest.

That keeps both things true at once. The history still reads as one change per commit, and
nothing is left at risk.

## The rest

- Commit after a real change. Not after a typo fix or a single command.
- Use `git commit -F -` with a heredoc for anything multiline. Quotes and specials break `-m`.
- No dashes and no semicolons in the message either.
- Say what changed, then why. The why is the part that is expensive to recover later.
- Surface the short hash in your status update.
- Push when JJ has asked for it. Creating a new remote branch is visible to other people, so
  say what is going up first.

Never reach for `--no-verify`, `git reset --hard`, deleting a lock file, or discarding a merge
conflict to make a symptom go away. Find the cause.

In a worktree, never use bare `git stash` or `git stash pop`. The stack is shared and another
session may pop your entry. Prefer a temporary commit.
