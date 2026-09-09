#!/usr/bin/env bash
# Guards bugs 35 and 36, which were both in the room services rather than in Rust.
#
# 35. A unit whose start timeout is shorter than the permission hook's own wait
#     can never let a permission be answered. It is killed while the hook is
#     still waiting, so the run neither succeeds nor fails cleanly.
# 36. flock refusing because the other watcher holds the lock is the design
#     working, not a failure, and exec made it the service's exit status.
set -uo pipefail
UNITS="$HOME/.config/systemd/user"
PORTAL="$HOME/Projects/AOS/AgenticOperatingSystem/carl/portal"
fail=0
say() { printf '  %s  %s\n' "$1" "$2"; }

hook=$(grep -ho '"timeout":[0-9]*' "$PORTAL"/../src/claude/*.rs 2>/dev/null | grep -o '[0-9]*' | sort -rn | head -1)
hook=${hook:-660}

for unit in carl-room.service carl-room-daily.service; do
    t=$(grep -oP 'TimeoutStartSec=\K[0-9]+' "$UNITS/$unit" 2>/dev/null)
    if [ -z "$t" ]; then say FAIL "$unit has no TimeoutStartSec"; fail=1
    elif [ "$t" -le "$hook" ]; then
        say FAIL "$unit waits ${t}s on a hook that waits ${hook}s"; fail=1
    else say ok "$unit outlives the ${hook}s hook at ${t}s"; fi
done

for s in room-watch.sh room-proactive.sh; do
    if grep -q 'exec flock' "$PORTAL/$s"; then
        say FAIL "$s uses exec flock, so a skip becomes the service's failure"; fail=1
    elif grep -q 'flock -n -E 75' "$PORTAL/$s" && grep -q 'eq 75' "$PORTAL/$s"; then
        say ok "$s tells a skip apart from a failure"
    else say FAIL "$s does not handle a held lock as a skip"; fail=1; fi
done

[ "$fail" -eq 0 ] && echo "room service guards passed" || echo "room service guards FAILED"
exit "$fail"
