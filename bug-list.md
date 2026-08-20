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
| 5 | The fix for bug 4 bound under a staging name that is longer than the real one, so a run directory whose socket path fitted under the 108 byte limit could still overflow it. | `aosd` refusing to start in a deep scratch directory, with `path must be shorter than SUN_LEN`. | `a_long_run_directory_still_binds` |

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
without the execute bit nobody else can reach any name inside it.

The first version of this fix also bound under a staging name and renamed into place. That
turned out to be both unnecessary and broken, and it is bug 5.

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


| 6 | The example policy the repo ships did not parse, because `plan_ttl_secs` sat below `[agents]` and a bare key belongs to the table above it, so it was read as an agent id. | Anybody copying `examples/policy.toml`, exactly as the file tells them to, gets a daemon that refuses to start. | `the_example_policy_parses`, `the_plan_lifetime_is_read_rather_than_defaulted` |
| 7 | `serve::run` booted the daemon before binding the socket, so a second `aosd` replayed the log and appended records before discovering a live daemon and exiting. It spent sequence numbers the live daemon believed were free, and every record that daemon wrote afterwards collided. | Reproduced: a second `aosd` printed "1 lost while unsupervised", appended a record, then failed with "a daemon is already listening", and the live daemon's next record reused the same number. | `a_second_writer_is_refused_rather_than_forking_the_sequence`, `reopening_a_damaged_log_never_goes_backwards`, `the_lock_is_released_when_the_ledger_is_dropped` |

## bug 5, in full

Caused by the fix for bug 4, which is the useful part of the story.

A unix socket path lives in a `sockaddr_un`, and that struct has a fixed field of about 108
bytes. Longer paths cannot be bound at all.

Bug 4's fix bound the socket under a staging name and renamed it into place, so that the real
path never existed with the wrong permissions. The staging name was
`.aosd.sock.<pid>.staging`, which is sixteen characters longer than `aosd.sock`. So a run
directory where the real socket path fitted with room to spare could still fail, and it did,
in a scratch directory the tests happened not to use.

Fix. No staging and no rename. Bind at the real path and chmod it.

That sounds like giving up the protection, and it is not, because the protection was never
the rename. The directory above the socket is set to `0700` first, and without the execute bit
nobody else can reach any name inside it however that name is chmodded. The directory is the
lock. The rename was guarding a window that the directory had already closed.

Guard. `a_long_run_directory_still_binds` builds a run directory whose socket path is just
inside the limit and binds there. Against the staging version it fails.

The lesson is about the shape of the mistake rather than about sockets. Two fixes in a row
reached for something clever, a umask and then a rename, when a plain directory permission was
both simpler and stronger. Worth asking, before adding a mechanism, whether something already
in place covers it.


## bug 6, in full

Found by running the shipped example rather than by reading it.

`examples/policy.toml` ended with this:

```toml
[agents]
# "wiper" = "deny"

plan_ttl_secs = 120
```

In TOML a bare key belongs to the table header above it, so that is not the plan lifetime. It
is an agent called `plan_ttl_secs` with the value `120`, and since an agent id has to be
lowercase letters, digits or dashes, the whole file was refused.

The file had never parsed. Nothing caught it because every test builds its policy in Rust,
so the example was only ever read by people, and reading it is exactly what does not reveal
the problem. The first person to copy it, doing precisely what the comment at the top tells
them to do, would have got a daemon that refuses to start.

Refusing is the correct behaviour for an unreadable policy, and that is what made this worse
rather than better. A supervisor that starts with a policy it could not read is a supervisor
enforcing nothing while looking healthy, so the strictness is right. It just meant the
failure landed on a new user in their first five minutes.

The fix moves the key above every table header. The guard is two tests. One loads the file
the repo actually ships and fails if it does not parse. The other checks the parsed value is
120, because the interesting failure is not a parse error but the key silently landing in
the wrong table.

The first attempt at that second test scanned the text for bare keys under a header, which
was wrong: `read = "allow"` under `[tiers]` is exactly that and is correct. The test failed
against the fixed file and caught itself. Checking the parsed structure is the only way to
see where a key actually landed.

## bug 7, in full

Three separate holes, all letting two processes write one log.

`serve::run` called `Daemon::boot` and then `listen::bind`. `bind` is the thing that enforces
one daemon per run directory, and `boot` replays the log and appends a
`lost_while_unsupervised` record for every agent it finds gone. So a second `aosd` did its
whole boot, wrote those records, and only then discovered a live daemon and exited.

By then it had spent sequence numbers the live daemon believed were still free. `Ledger::open`
caches `next_seq` once, so the live daemon never noticed, and every record it wrote from that
point carried a number already in the file.

That is the worst shape of corruption available here. Two writers tearing a line would at least
produce something unparseable that a reader can complain about. Forking the numbering produces
records that are all individually well formed, in a file that is no longer an ordering, and
nothing downstream can tell.

Fix, in three parts, because closing only the route that was reported would leave the same
failure reachable by other doors.

`bind` moved above `boot`. Nothing touches the ledger until this process has proved it is the
only supervisor for that directory. That closes the reported route.

`next_seq` comes from the maximum sequence in the file rather than from the last record. `last`
assumes the file is in order, and the reason this bug exists is a file that was not. Taking the
maximum means a reopen can only move forwards, so a log that has already been damaged stops
getting worse.

`Ledger::open` takes an exclusive `flock`, non blocking, and refuses if it cannot have it. That
closes every remaining door, including `aos run` against a directory a daemon already owns.
`flock` is per open file description, so the lock lives exactly as long as the `Ledger` and is
released by the kernel if the process dies, which is why this rather than a lock file: there is
nothing to clean up after a crash.

**A real behaviour change, worth knowing before this merges.** `aos run` against a run directory
that a daemon already owns now fails, where before it appeared to work. It never really worked:
that is the exact case that forked the numbering. But anybody in the habit of doing it will see
a new error.

An existing test also had to change. `sequence_continues_across_reopening` opened a second
`Ledger` while the first was still in scope, to stand for a restart. With the lock that is
refused, and correctly: holding both at once is not a restart, it is two daemons. It now drops
the first, which is what a restart actually is, and the case it used to accidentally cover has
its own test.

Verified end to end. A second `aosd` on a live directory now fails at `bind`, writes nothing,
and leaves the log at two records with no duplicate numbers. Before the fix it wrote a third
and the live daemon then reused that number.
