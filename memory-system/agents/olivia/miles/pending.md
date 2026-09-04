# Waiting on JJ, from the mail

Held back from this public copy.

The real file is Miles's own list of things in the inbox that only JJ can finish: sending a
draft he wrote, answering a calendar invite, granting a Drive share. It is in the private
repository `JJtmc1234/army-memory`.

It is held back for the same reason as `ACTION-ITEMS.md`. The entries name teachers, the
classes they set and when they meet, and the people asking for access to things.

## Why it exists, which is the part worth publishing

These used to go on `Projects/ACTION-ITEMS.md`, the list of everything only JJ can do. A report
every two hours filled it with RSVPs and unsent drafts, and a list that is mostly routine mail
is a list nobody reads to the bottom of. JJ asked on 2026 09 04 for email things to stay off
it, so they come here instead and his list is for everything else.

Nothing is dropped by the split. A needed reply that exists only as a sentence in a report is
how work goes missing, which is the reason this mechanism was built. Miles is handed both lists
in his prompt so he never raises the same thing twice, and `run.sh` checks both before
appending, so a line moved up to JJ's list by hand does not come back here on the next run.

The rules are in `shared/work/action-items.md`, which is not held back.
