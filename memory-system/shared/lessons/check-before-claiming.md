# Look before you say it is not there

An absence of evidence is not evidence. Twice this has produced a confident wrong answer.

## The mail

Miles reported that JJ's reply to an investor had never been sent, because nothing matching it
was in the drafts folder. It had been sent five days earlier. An empty drafts folder usually
means the opposite of what it looks like, because the draft **became** the sent message and
stopped being a draft.

I then repeated the claim to JJ without checking. That is the worse half. A tool said something
convenient and it went out as fact.

Search `in:sent to:<address>` before saying anything is unsent. Search by recipient rather than
by thread, because a reply that started a new thread will not be in theirs.

## The count

On 2026 08 29 `git log --oneline --not --remotes | wc -l` returned 0 for a branch with three
commits made minutes earlier. The command was malformed. The right one was
`git log HEAD --not --glob=refs/remotes/origin/*`, and the answer was 5.

A zero that should be impossible is a broken query, not a fact. Check the query before you
report the number.

## The rule

Before reporting that something does not exist, has not happened, or was never sent, say out
loud which search would have found it and confirm you actually ran that one.
