#!/bin/bash
# Miles, run on a trigger rather than by hand.
#
# The tool list is the safety property, and as of 2026 08 29 it draws the line in a different
# place. Miles can search, read, draft, reply and send. He has no trash, no spam marking and no
# label changing, so a bad turn can embarrass JJ but cannot lose him a message he needed. An
# agent that can delete mail and has been told not to is one bad turn away from deleting mail.
#
# Sending is the part JJ authorised knowingly, having been shown what a live auto responder on a
# real mailbox can do. The rules that keep it sane live in Projects/MEMORY/work/mail.md.
#
# The rules he must follow are put into his prompt rather than left on disk for him to fetch.
#
# On 2026 08 29 he reported: "the gibberish sweep rules are meant to be at
# Projects/MEMORY/mail.md. That file does not exist and the path is outside my working
# directory, so I ran no sweep and sent nothing." Two faults in one sentence. The path was
# wrong, and even the right path is outside the directory he runs in, so he could not have read
# it either way. It failed silently for four days.
#
# Handing him the text closes both. There is no path to go wrong and no directory to be outside
# of. The check below refuses to run at all if a rules file is missing, because a run that
# quietly skips the safety rules is worse than a run that does not happen.
#
# Read and Write are for his own directory: his memory files and the report. He does not write
# to ACTION-ITEMS.md himself, because that file is shared and he cannot reach it. He prints
# ACTION lines and this script appends them, which is one writer instead of several.
set -uo pipefail

HOME_DIR="$HOME/Projects/army/olivia/miles"
OUT="$HOME_DIR/reports"
mkdir -p "$OUT"
STAMP=$(date '+%Y-%m-%d_%H%M')
REPORT="$OUT/report-$STAMP.md"

# Read, compose and send. No trash, no spam marking, no label changes: the tools that destroy
# are still absent, so a bad turn can embarrass JJ but cannot lose him a message he needed.
#
# This prompt used to end with "do not send anything", which contradicted every other thing
# Miles is told. He has the send tools, his brief says JJ authorised sending on 2026 08 29, and
# mail.md sets out exactly which sends need no asking. The timer is the only thing that ever
# runs him, so a blanket refusal here meant a reply he was told to write could never be written,
# and mail waiting on him would have waited forever. It now defers to mail.md, which is the one
# place the bounds live, and anything outside those bounds becomes a draft and an action item
# rather than nothing at all.
MAIL_TOOLS="mcp__claude_ai_Gmail__search_threads,mcp__claude_ai_Gmail__get_thread,mcp__claude_ai_Gmail__get_message,mcp__claude_ai_Gmail__list_drafts,mcp__claude_ai_Gmail__list_labels,mcp__claude_ai_Gmail__create_draft,mcp__claude_ai_Gmail__update_draft,mcp__claude_ai_Gmail__send_message,mcp__claude_ai_Gmail__reply,Read,Write"

MAIL_RULES="$HOME/Projects/MEMORY/work/mail.md"
REPORT_RULES="$HOME/Projects/MEMORY/work/reporting.md"
ACTIONS="$HOME/Projects/ACTION-ITEMS.md"
# Where Miles's own findings go.
#
# They used to go on JJ's list. A report every two hours filled it with RSVPs and unsent drafts,
# and a list that is mostly routine mail is a list nobody reads to the bottom of. JJ asked for
# email things to stay off it on 2026 09 04.
#
# Still appended somewhere rather than dropped. A needed reply that exists only as a sentence in
# a report is how work goes missing, which is the reason this mechanism was built at all.
PENDING="$HOME/Projects/army/olivia/miles/pending.md"

for needed in "$MAIL_RULES" "$REPORT_RULES" "$ACTIONS" "$PENDING"; do
  if [ ! -f "$needed" ]; then
    echo "miles: $needed is missing, so the run is refused" >&2
    echo "miles: sending is bounded by those files and a run without them is unbounded" >&2
    exit 1
  fi
done

cd "$HOME_DIR" || exit 1

timeout 900 claude -p \
  --append-system-prompt "$(cat "$HOME_DIR/brief.md")

# The mail rules, in full. These bound what you may send.

$(cat "$MAIL_RULES")

# The scheduled report rules, in full.

$(cat "$REPORT_RULES")

# JJ's action items as they stand. Anything ticked is done, so leave it out of the report.
# These are his, not yours. You never add to this list: it is here so you do not raise
# something he has already dealt with.

$(cat "$ACTIONS")

# What you have already found and are waiting on him for. Same rule: do not raise these again.

$(cat "$PENDING")" \
  --allowedTools "$MAIL_TOOLS" \
  --permission-mode acceptEdits \
  "Triage JJ's Gmail inbox. Search 'in:inbox is:unread newer_than:2d' first. What comes back is
the report, because JJ has not seen it yet. Then search 'in:inbox is:read newer_than:7d' and
carry a read message over only for the three exceptions in your brief. Leave out anything already ticked in the action items in your system prompt, and run the
gibberish sweep exactly as the mail rules there describe it. Apply his importance rules.

Sending. Follow the mail rules in your system prompt and nothing else. They say which sends you
make without asking, and they say you ask for the rest. Where a message needs a reply those
rules do not already cover, write it as a draft with create_draft and put the draft id in the
report. Never leave a needed reply as a sentence in a report and nothing else. That is how work
goes missing.

Anything only JJ can do, including sending a draft you wrote, goes at the very end of the
report as its own line starting with the word ACTION and a colon, in the action item format
from the rules above. This script appends those lines to your own pending file. Do not try to
edit either list yourself: one writer is safer than several.

Both lists are in your system prompt above. If a thing is already on either of them, do not
print an ACTION line for it and do not put it in the report again.

You still cannot delete, trash, archive or relabel anything, and you have no tool for any of it.

The report. Lead with what needs somebody to do something. Never lead with a count of unread
mail: a number reads as a queue of work and this report is the work already done. If nothing
needs doing, say that in the first sentence and say it plainly. Follow the scheduled report
rules in $HOME/Projects/MEMORY/work/reporting.md.

Write the report to $REPORT using the Write tool. Then print the report to stdout so the caller
can forward it." \
  > "$OUT/last-run.log" 2>&1

STATUS=$?
if [ $STATUS -ne 0 ]; then
  echo "miles: the run failed with status $STATUS, see $OUT/last-run.log" >&2
  exit $STATUS
fi

# Anything only JJ can do goes on his list, appended here rather than by Miles.
#
# One writer. The list is shared by every agent and they all append to the bottom of it, so a
# whole file write from any one of them loses what another added in between.
if [ -f "$REPORT" ]; then
  while IFS= read -r line; do
    item="- [ ] $(echo "${line#ACTION:}" | sed 's/^ *//')"
    # Not twice. A report is written every two hours and the same reply can need sending in
    # several of them before JJ gets to it.
    # The double dash matters. An action item starts with a dash, and without it
    # grep reads the line as its own options, the check always fails, and the
    # same item is appended every two hours forever.
    # Not twice, and not on either list. The same reply can need sending in several reports
    # before JJ gets to it, and a line that was moved to his list by hand must not come back
    # here on the next run.
    if ! grep -Fqx -- "$item" "$PENDING" && ! grep -Fqx -- "$item" "$ACTIONS"; then
      printf '%s\n' "$item" >> "$PENDING"
      echo "miles: added to miles/pending.md: $item" >&2
    fi
  done < <(grep '^ACTION:' "$REPORT" || true)
fi

# Straight to Slack, in real time, which is what the report is for.
#
# Its own channel rather than #general. A report every two hours drowns a shared channel, and a
# channel people mute is a report nobody reads. Set MILES_CHANNEL to move it.
CHANNEL="${MILES_CHANNEL:-#miles}"
FALLBACK="#general"
if [ -f "$REPORT" ]; then
  SUMMARY=$(head -c 1200 "$REPORT")
  BODY="Miles, inbox report $STAMP

$SUMMARY"
  # The report must never vanish. A dedicated channel is the goal, but silence is worse than
  # the wrong channel: an earlier version reported success while Slack had refused it, and
  # nobody noticed for days.
  if ! "$HOME/.local/bin/carl" say "$CHANNEL" "$BODY" >> "$OUT/last-run.log" 2>&1; then
    echo "miles: $CHANNEL refused the report, falling back to $FALLBACK" >&2
    "$HOME/.local/bin/carl" say "$FALLBACK" "$BODY

Miles could not post to $CHANNEL. Create it and run /invite @Carl to stop these landing here." \
      >> "$OUT/last-run.log" 2>&1 || echo "miles: could not reach Slack at all" >&2
  fi
  echo "$REPORT"
else
  echo "miles: no report was written" >&2
  exit 1
fi
