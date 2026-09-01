#!/bin/bash
# Miles, run on a trigger rather than by hand.
#
# The tool list is the safety property, and as of 2026 08 29 it draws the line in a different
# place. Miles can search, read, draft, reply and send. He has no trash, no spam marking and no
# label changing, so a bad turn can embarrass JJ but cannot lose him a message he needed. An
# agent that can delete mail and has been told not to is one bad turn away from deleting mail.
#
# Sending is the part JJ authorised knowingly, having been shown what a live auto responder on a
# real mailbox can do. The rules that keep it sane live in Projects/MEMORY/mail.md.
#
# Read was added so Miles can check ACTION-ITEMS.md before reporting. It is filesystem read
# only, and Write still exists solely to put the report on disk.
set -uo pipefail

HOME_DIR="$HOME/Projects/army/olivia/miles"
OUT="$HOME_DIR/reports"
mkdir -p "$OUT"
STAMP=$(date '+%Y-%m-%d_%H%M')
REPORT="$OUT/report-$STAMP.md"

# Read, compose and send. No trash, no spam marking, no label changes: the tools that destroy
# are still absent, so a bad turn can embarrass JJ but cannot lose him a message he needed.
MAIL_TOOLS="mcp__claude_ai_Gmail__search_threads,mcp__claude_ai_Gmail__get_thread,mcp__claude_ai_Gmail__get_message,mcp__claude_ai_Gmail__list_drafts,mcp__claude_ai_Gmail__list_labels,mcp__claude_ai_Gmail__create_draft,mcp__claude_ai_Gmail__update_draft,mcp__claude_ai_Gmail__send_message,mcp__claude_ai_Gmail__reply,Read,Write"

cd "$HOME_DIR" || exit 1

timeout 900 claude -p \
  --append-system-prompt "$(cat "$HOME_DIR/brief.md")" \
  --allowedTools "$MAIL_TOOLS" \
  --permission-mode acceptEdits \
  "Triage JJ's Gmail inbox. Search 'in:inbox is:unread newer_than:2d' first. What comes back is
the report, because JJ has not seen it yet. Then search 'in:inbox is:read newer_than:7d' and
carry a read message over only for the three exceptions in your brief. Read
$HOME/Projects/MEMORY/mail.md and $HOME/Projects/ACTION-ITEMS.md, leave out anything already
ticked in the action items, and run the gibberish sweep exactly as mail.md describes it. Apply
his importance rules. Write the report to $REPORT using the Write tool. Then print the report to stdout so the
caller can forward it. Do not send, delete, trash or archive anything." \
  > "$OUT/last-run.log" 2>&1

STATUS=$?
if [ $STATUS -ne 0 ]; then
  echo "miles: the run failed with status $STATUS, see $OUT/last-run.log" >&2
  exit $STATUS
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
