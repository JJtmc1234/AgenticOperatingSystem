# bug list

Every bug found, and the test that stops it coming back.

A bug is not finished when it is fixed. It is finished when a test exists that fails against
the old code. No entry goes in here without that test, and the test name is written down so
anyone can run it.

To check an entry is real, revert the fix, run the named test, and watch it fail.

| id | bug | found by | guard |
|---|---|---|---|
| 1 | Agent output went to a pipe nobody drained. Output was lost, and any agent writing past the roughly 64 KB pipe buffer blocked forever. | Running `aos run examples/hello.json` by hand. The echo printed nothing. | `a_chatty_agent_finishes_and_its_output_is_kept` and `agent_output_lands_in_its_log` |
| 2 | The adoption test helper waited on `setsid` with no bound. `setsid` had not forked, so the sleeper was still our child and the wait blocked for the full 511 seconds. | `cargo test --test adoption` hitting a two minute timeout with no output. | `wait_bounded` in `tests/adoption.rs`, which panics with the reason after 5 seconds |
| 3 | The daemon test harness killed the daemon on drop but not its agents, so a failing test left real processes running on the machine. | Three stray `sleep 400` processes surviving a failed `cargo test`. | `Drop for Aosd` in `tests/daemon.rs` now sends `stop_all` before killing the daemon |
| 4 | `bind` narrowed the process wide umask across the bind call, so any other thread creating a directory at that moment got one with no execute bit and could not use it. | A flaky `Permission denied` in `cargo test`, one run in three. | `binding_concurrently_does_not_disturb_other_threads` |

## bug 1, in full

`Supervisor::start` spawned children with `Stdio::piped()` for stdout and stderr, then never
read either pipe. Two problems came from one mistake.

The visible one was that output vanished. The pipe was dropped when the child was reaped, so
an agent could run perfectly and leave no trace of what it said.

The serious one was the deadlock. A pipe holds about 64 KB on Linux. Once full, the next
write blocks. With nothing reading, a chatty agent stops there and never exits, and the
supervisor waits on it forever. This was found by luck, because `echo` writes 30 bytes.

Fix. Children now write to a per agent file under the run directory, opened for appending.
A file never fills, so there is nothing to drain and nothing to block on.

Guard. `a_chatty_agent_finishes_and_its_output_is_kept` runs `seq 1 200000`, about 1.3 MB,
which is twenty times the buffer. Against the piped version it hangs until its 20 second
deadline and fails. `agent_output_lands_in_its_log` checks the content is actually kept,
because a fix that stops the hang but still loses output is only half a fix.

Verified by reverting the fix and watching the test fail, then restoring it.

## bug 2, in full

In test code rather than in the product, and worth recording anyway, because the trap is real
and the lesson is one this project already claims to have learned.

`make_orphan` needs a process that is genuinely not our child. It ran `setsid sleep 511` and
waited for `setsid` to exit, expecting to be left with an orphan.

`setsid` only forks when it is not already a process group leader. Otherwise it execs its
argument in place. The spawned `setsid` was not a group leader, so it became `sleep` itself,
and the wait sat there for the full 511 seconds. The test suite hit its timeout with no
output at all.

Two fixes, because there were two mistakes.

`setsid --fork` forces the fork, so the orphan is real. That is the correctness fix.

`wait_bounded` gives the wait a five second deadline and panics naming the cause. That is the
guard, and it is the more important half. The project already holds the rule that an
unbounded wait is a bug, in `signal::wait_bounded` and in the Windows AOS bug list before
that. The rule was written down and then broken in the very next file, which is the lesson
here. A hang reports nothing. A bounded wait that fails tells you what went wrong.

## bug 3, in full

Also test code, and this one escaped onto the machine, which makes it worth more than the
usual test bug.

`Drop for Aosd` killed the daemon so no test could leave one running. That is right for the
daemon and wrong for its agents, because AOS is deliberately built so that killing a daemon
leaves its agents alive. Correct in production, a process leak in a test. Three `sleep 400`
processes outlived a failing run and were still there afterwards.

Fix. Drop sends `stop_all` first, then kills the daemon. The order matters and it is the
reverse of the one that looks natural.

The cleanup itself must not panic. `Drop` can run while a test is already unwinding, and a
panic there aborts the process and hides the real failure. That is why `try_ask` exists
alongside `ask`.

Guard. The harness now cleans up on every path, so a future failure leaves nothing behind.
Checked by counting `sleep` processes before and after a full run, zero both times.

The rule worth keeping. A test that starts a real process owns it until that process is gone,
including when the test fails. Especially when the test fails.


## bug 4, in full

The first bug in this list that is a real concurrency fault rather than a mistake in a test.

The socket must be reachable only by its owner. Creating it and then tightening it leaves a
window where anyone could connect, so the obvious move is to narrow the umask across the bind
and put it back afterwards. That is what the code did.

`umask` is per process, not per thread. While one thread sat inside `bind` with the umask at
`0o177`, every other thread in the program was affected too. A directory created in that
window comes out `0600` rather than the usual mode, and `0600` on a directory has no execute
bit, so nothing can be created inside it. The other thread's own bind then failed with
`Permission denied`.

It showed up as a flake, roughly one full test run in three, in a test that had nothing to do
with permissions. That is what a shared mutable global looks like from the outside.

Fix. No umask at all. The parent directory is set to `0700`, which is the real lock, because
without the execute bit nobody else can reach any name inside it. The socket is then bound
under a staging name, chmodded to `0600`, and renamed into place. Rename is atomic, so the
real path never exists with the wrong mode for even an instant. The listener keeps working
across the rename because it is bound to the inode, and the new name resolves to that inode.

Guard, and it took three attempts to get right, which is the more useful story.

The first version had eight threads each binding and checking their own socket. It caught the
bug two runs in five. A guard that fires half the time is not a guard.

The second version added more rounds and got to four in eight. Still a coin flip, because a
binder only notices the bug when its own bind happens to fail, and the window is narrow.

The third version splits the work in two. Binder threads do nothing but call `bind`. Watcher
threads do nothing but create a directory and check its mode, which samples the bad window
far more often than waiting for a bind to fail. That catches it every time.

It also asserted the wrong thing at first. It expected new directories to be `0700`, and on
this machine the ambient umask is `0002` so they are `0775`, and the test failed against
correct code. The invariant is not "directories come out 0700". It is "binding does not change
what other threads get", so the baseline is now measured before the threads start rather than
assumed.

Verified ten runs against the fix, all passing, and ten against the restored bug, all failing.
