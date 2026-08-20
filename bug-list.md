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
| 7 | `start_token` collapsed every `/proc` read failure into `None`, and `None` meant "the process is gone". A comm field that is not UTF-8 makes `read_to_string` fail, so a live agent was written off, dropped from `believed_running` on every later boot, and put beyond `stop` and `stop-all`. An agent could escape the kill switch by renaming itself. | Reproduced on this machine with `prctl(PR_SET_NAME, "\xff\xfe-agent")`. | `a_name_that_is_not_utf8_does_not_hide_the_start_time`, `an_agent_that_cannot_be_checked_is_neither_adopted_nor_written_off`, `a_gone_agent_is_still_lost_rather_than_unknown` |
| 8 | Adoption trusted a pid and start token and nothing else. A token counts ticks since boot, so the pair means nothing across a reboot, and adoption never checked what the pid was running nor whether it is still allowed. A stale record could adopt a stranger, which `stop_all` then SIGKILLs. | Reproduced by pointing a log at a live process the supervisor never started: `alive` of 1, `is_adopted` true, and `stop` killed it. On this machine 87 processes share start token 18. | `a_handle_from_another_boot_is_lost_rather_than_matched`, `refuses_to_adopt_a_pid_running_a_different_program`, `refuses_to_adopt_a_program_the_allowlist_no_longer_permits` |

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

`ok()?` threw away the difference between "not there" and "could not look", and the whole
recovery path was built on that difference without anybody noticing it was missing.

`start_token` read `/proc/<pid>/stat` with `read_to_string` and turned any failure into `None`.
`is_still` turned `None` into "not our process". `recover` turned that into `lost`. Boot writes
a `lost_while_unsupervised` record for everything in `lost`, and `believed_running` folds over
that record, so the agent is gone from the log's belief on every later boot as well. It is never
adopted, and it is unreachable by `stop` and by `stop-all`, which is the kill switch.

The read that fails needs no privilege and no unusual conditions. The kernel escapes only
newline and backslash in the comm field, so bytes above 0x7f pass through raw and the stat line
is simply not valid UTF-8. `read_to_string` refuses it. So **an agent that wanted to survive the
kill switch only had to rename itself**, which is the sharpest way to put what this bug was.
Descriptor exhaustion inside `aosd` reaches the same end by a different route.

Fix, in three parts, matching the three places the information was lost.

`parse_start_token` works on bytes. It finds the last `)` in the bytes and only converts what
follows, which is fixed width ASCII, so a name that is not UTF-8 no longer hides the start time.

`started` returns three answers rather than two: `At`, `Gone` for `ENOENT`, and `Unknown` for
anything else. `start_token` and `is_still` are kept on top of it for callers about to send a
signal, where all three failures do mean the same thing: do not touch that pid. Deciding whether
to *write an agent off* is a different question and now has a different function.

`Recovered` grows an `unknown` list, and boot neither adopts those agents nor records them as
lost. It prints a warning naming the pid instead. Both other answers are actively wrong:
adopting means signalling a pid nothing confirmed, and recording it lost is the bug. Leaving the
log saying it is running is the only claim still true, and a person is told to look.

Guard. The parsing test builds a stat line with high bytes in the comm field and asserts it is
genuinely not UTF-8 before asserting field 22 still comes out, so it cannot quietly stop testing
what it was written for. The replay tests cover both directions: an agent that cannot be checked
is in neither list, and one that is genuinely gone is still lost rather than swallowed by the
new case.

Verified by putting the UTF-8 first parse back. The non UTF-8 test failed and every other test
passed, which is the right shape.

## bug 8, in full

The comment on `ProcessHandle` said the pid and token pair is unique "for as long as the machine
has been up". That was true, documented, and unenforced, and the run directory outlives a boot.

So a record written before a power loss could be compared against a machine that had since
restarted, where the pair means nothing. Early boot is where that bites rather than being a
remote possibility: the startup sequence is mostly deterministic and mostly runs in the same few
ticks, so on this machine 87 processes share start token 18 and 14 share token 19. A stale
handle matching a stranger is adopted under an agent id, and `stop_all` then SIGKILLs it.

There is a second harm that needs no collision at all, and it is the easier one to trigger.
Adoption never looked at what the pid was running. `Event::Started` records the program and
nothing ever read it back. Nor did adoption consult the allowlist. So taking a program off
`allowed-programs.json` and restarting the daemon adopted the running agent straight back in,
under a rule that no longer permits it.

Fix, in three parts.

`ProcessHandle` carries the boot, from `/proc/sys/kernel/random/boot_id`. Optional, so a log
written before this field existed still reads, and `None` means the boot is unknown rather than
known to match. A different boot is `Gone`, because that agent certainly did not survive. An
unknown boot is `CannotTell`, so it is neither adopted nor written off, which is the bucket bug
7 added. Writing those off instead would lose a live agent on the first upgrade.

`adopt_from` compares the recorded program against `readlink /proc/<pid>/exe`, canonicalising
both so a symlinked path does not read as a mismatch, and checks the program is still on the
allowlist. Neither check passing means not adopted.

A refused agent moves to `unknown` rather than being dropped or called lost. Something is on
that pid and this supervisor will not touch it, which a person needs to know, and calling it
lost would write a record saying it ended when it may well be running.

That cost `ProcessHandle` its `Copy`, which rippled through about a dozen call sites. Worth it:
the boot is part of the identity, so it belongs in the handle rather than beside it, and a
handle that can be copied around freely is part of how the identity got treated as smaller than
it is.

Guard. Two tests on the boot, one for a different boot being lost and one for a missing boot
being unknown. Two on the program, both driving a real live process the supervisor never
started, which is how the issue reproduced it. Verified by disabling the program check: both
program tests failed and the five existing adoption tests kept passing.

Not covered: an actual reboot. The boot check is exercised with two synthetic ids rather than by
restarting the machine, which no test can arrange.
