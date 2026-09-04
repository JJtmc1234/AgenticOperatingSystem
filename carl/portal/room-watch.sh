#!/bin/bash
# Carl, reacting to the room.
#
# Runs on a timer, costs nothing when the room is quiet, and only spends a model call when
# somebody who is not Carl has actually said something.
#
# **The watermark always moves, even when no handoff happens.** That is what stops the loop.
# `carl portal read` consumes the new messages and advances `seen` in ~/.carl/portal.json, so
# Carl's own reply is already behind the watermark by the next tick. A version that handed the
# messages over without consuming them would find the same ones two minutes later, hand them
# over again, and keep doing that for as long as the timer runs.
#
# The messages are put into his prompt rather than left for him to fetch, which is the same
# choice Miles' runner makes and for the same reason. There is no path to get wrong and no
# second read that could come back different from the one this script already checked.
set -uo pipefail

CARL="$HOME/.local/bin/carl"
LOG="$HOME/.carl/room/watch.log"
mkdir -p "$(dirname "$LOG")"

say() { printf '%s  %s\n' "$(date '+%Y-%m-%d %H:%M:%S')" "$1" >> "$LOG"; }

[ -r "$HOME/.carl/portal.json" ] || { say "no portal.json, nothing to watch"; exit 0; }

# Consume whatever is new. This is the step that moves the watermark.
NEW=$("$CARL" portal read 2>&1)
STATUS=$?
if [ $STATUS -ne 0 ]; then
  say "could not read the room: $NEW"
  exit 0            # a room that is down is not a failed unit, it is a quiet tick
fi

case "$NEW" in
  "Nothing new in the room."*|"") exit 0 ;;
esac

# Carl talking to himself is not a reason to wake Carl. His own lines are already past the
# watermark now, so dropping them here costs nothing and saves a model call every time he posts.
OTHERS=$(printf '%s\n' "$NEW" | grep -v '^Carl:')
[ -n "$OTHERS" ] || { say "only Carl's own messages, no handoff"; exit 0; }

say "new in the room, handing to Carl: $(printf '%s' "$OTHERS" | head -c 200)"

# Only ever one Carl in the room at a time. Two runs overlapping would answer the same person
# twice and each would think it was the only one.
exec flock -n "$HOME/.carl/room/.lock" "$CARL" handoff --from jj --to carl "New messages in the room:

$OTHERS

Reply with \`carl portal say <words>\` only if it is addressed to you, asks you something, or
you know something the room does not. Otherwise say nothing at all and end your turn. Silence is
a normal outcome here and it is better than filling a shared record with acknowledgements.

At most one message. Do not reply to your own earlier lines. Everything you say is kept and
Hunter reads it, so say what is true rather than what sounds busy."
