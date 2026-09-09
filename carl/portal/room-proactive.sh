#!/bin/bash
# Carl, starting something rather than answering it.
#
# Once a morning. The reactive watcher only ever fires when somebody else speaks first, so
# without this Carl can never raise a thing nobody thought to ask him about, which is most of
# what a chief is for.
#
# Saying nothing is a normal outcome and the prompt says so twice. A daily agent that feels it
# has to produce a message produces one whether or not there is anything in it, and a room where
# most lines are "no update" is a room people stop reading. The cost of a quiet run is one small
# model call. The cost of a noisy one is the record being worth less.
set -uo pipefail

CARL="$HOME/.local/bin/carl"
LOG="$HOME/.carl/room/watch.log"
mkdir -p "$(dirname "$LOG")"

printf '%s  proactive run\n' "$(date '+%Y-%m-%d %H:%M:%S')" >> "$LOG"

[ -r "$HOME/.carl/portal.json" ] || exit 0

# Shares the watcher's lock, so this never lands while Carl is mid reply to somebody.
# Not exec, and -E, so that "somebody else has the lock" can be told apart
# from "the work failed". Sharing the lock is the whole design here: this
# must never land while Carl is mid reply. Exiting non zero for it made
# systemd call an ordinary quiet skip a failed service, once an hour.
flock -n -E 75 "$HOME/.carl/room/.lock" "$CARL" handoff --from jj --to carl "Your daily look at the room.

Run \`carl portal read --all\` to see what is there, then decide whether you have anything worth
saying that nobody has asked you for. Something finished, something stuck, something JJ or
Hunter would want to know before they ask.

If you do, say it once with \`carl portal say <words>\`, in one or two sentences, and say whose
work it was. If you do not, say nothing at all and end your turn. Most days that is the right
answer, and a room full of updates that say nothing is worse than a quiet one.

Never post a status update just because this ran."

status=$?
if [ "$status" -eq 75 ]; then
    printf '%s  skipped, the watcher is mid reply\n' "$(date '+%Y-%m-%d %H:%M:%S')" >> "$LOG"
    exit 0
fi
exit "$status"
