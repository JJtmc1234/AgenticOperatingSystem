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
| 7 | Three of the daemon's four ledger appends threw the error away with `let _ =`, so a refusal or a stop could go unrecorded while the caller was told everything worked. | Reading `crates/aosd/src/daemon.rs` against its own module doc, which claimed every mutation is written down first. | `a_refusal_that_could_not_be_written_reports_both`, `a_plan_that_could_not_be_recorded_is_not_offered`, `a_stop_that_could_not_be_recorded_is_reported_as_unrecorded`, `stop_all_reports_an_agent_it_stopped_but_could_not_record` |
| 8 | `aos run` started the child, then appended the `Started` record with a bare `?`, so a log that would not take the write left a process running that nothing had recorded. | Reading `crates/aos-cli/src/runtime.rs` against the comment directly above it, which said "Append, then act". | `a_start_that_cannot_be_recorded_leaves_no_surviving_child` |
| 9 | `aos run` started an agent with no policy check at all. Only the daemon called the gate, so a policy denying every tier was ignored by one of the two ways to start an agent. | Never fired. Found by reading, then reproduced: with every tier plus `hello` set to deny, `aos run` started the child, exited 0, and wrote a `Started` record. | `a_denying_policy_refuses_aos_run`, `a_prompt_tier_refuses_aos_run_and_points_at_the_daemon` |
| 10 | `Ledger::append` called `flush` on a `std::fs::File`, which does nothing, because a `File` holds no userspace buffer. Every record stopped at the page cache, so a power loss could drop a `Started` record for a process that was genuinely running. | Never fired. Found by reading `append` against the readme's claim that the log is durable. | `every_append_is_synced_after_it_is_written`, `a_failed_sync_fails_the_append_and_does_not_burn_the_number` |
| 11 | `believed_running` treated `Event::Refused` as an ending, so refusing a start because the agent was already running erased that live agent from the log's belief. Nothing could then find, adopt or stop it, including the kill switch. | Never fired. Reproduced: a second `aos start` made `aos status` report nothing running while the process was alive, and `stop-all` said "nothing was running". | `a_refusal_does_not_erase_an_agent_that_is_already_running`, `a_refusal_after_an_exit_leaves_the_agent_ended` |

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

`Daemon` wrote four kinds of record. One of them checked whether the write worked.

`launch` got it right. It appends the `Started` record, and if the append fails it stops the
process again, because a running agent nobody wrote down is a running agent nobody can find.
The other three used `let _ = self.ledger.append(...)` and carried on: `Planned` in the gate,
`Refused` in `record_refusal`, and `Stopped` in both `stop` and `stop_all`.

Each one loses something different.

A dropped `Refused` record is the worst of them, because refusing out loud is the entire
reason the audit log exists. The caller still got a normal refusal message, so nothing
anywhere said the refusal had not been written. Later, a log with no entry looks exactly like
a call that was never made.

A dropped `Stopped` record is the one that can hurt the machine. `believed_running` in
`aos-core/src/fold.rs` is a fold over the log, so with no `Stopped` record the log keeps
calling the agent running. The next boot reads that and goes looking for the pid. Linux reuses
pids, so the process it finds may belong to somebody else entirely. The existing start token
check is what stops that becoming a signal to a stranger, which means the guard held, but only
because a different guard was doing its job.

A dropped `Planned` record just loses the audit trail of an offer, since plans live in memory
only. It is the least harmful and it was fixed the same way.

Fix. `record_refusal` became `refuse`, which returns the `Response` instead of returning
nothing. That is the part worth keeping. Writing the record and answering the caller are now
one operation, so a future call site cannot record a refusal and then forget to mention the
write failed, and three call sites got shorter. `Planned` now refuses to offer a plan it could
not write down, which is safe because nothing has run at that point. `stop` and `stop_all`
report the agent as stopped, which is true, and also report that the log does not say so,
which is the part the operator has to fix by hand. A stop cannot be undone, so reporting both
facts is the only honest option.

The module doc was also wrong, and its wrongness is why this was worth writing up. It said
every mutation appends before the supervisor is told. That cannot be true for starting or
stopping. A pid and its start token do not exist until after the spawn, and an exit code does
not exist until after the stop, so there is nothing truthful to write beforehand. The real
rule those two follow is that a mutation which could not be recorded is undone or reported as
unrecorded, never dropped. The doc now says that instead.

Guard. Four tests, one per site, in a new `mod tests` inside `daemon.rs`. They build a
`Daemon` by hand over a ledger whose every write fails, which needed a seam that did not
exist: `Ledger` now holds a `Box<dyn Write>` rather than a `File`, and `Ledger::to_sink` makes
one over anything. That seam is the more valuable half of this fix. Every belief the runtime
holds is a fold over the log, so "the log refused a write" is the failure most worth testing,
and it is exactly the one a real file will not reproduce on demand. Before this, no test in
the project could reach any of that code.

The daemon tests in `tests/daemon.rs` still drive the real binary over a real socket. These
new ones sit inside the crate instead, because `boot` needs a log it can actually open and the
whole point here is a log that cannot be written.

Verified by rebuilding `daemon.rs` as the committed version plus the four new tests and
running them. All four failed, each for its own reason: the plan test got `PlanRequired` back,
the stop test got `Stopped`, `stop_all` reported an empty `failed` list, and the refusal test
got a message that mentioned only the policy. Then the fix was restored and all four passed,
with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and the full `cargo test`
clean.

## bug 8, in full

`aos run` spawned the agent, then appended the `Started` record with a bare `?`. If the append
failed, the `?` returned immediately, the `Supervisor` was dropped, and the child kept running.
Dropping a `std::process::Child` does not kill anything, and there is no `Drop` on `Supervisor`,
so the process simply stayed.

That is the worst shape a failure can take in this system. The record is the only thing that
would have carried the pid, so with no record there is nothing for `aos status` to reconcile,
nothing for the next `aosd` to adopt, and nothing for the kill switch to stop. The agent is
running and no part of the machine knows it exists. Confirmed rather than reasoned about: the
old code left `/usr/bin/sleep 4919.3543467` as pid 3543469, with an 82 minute lifetime ahead of
it and no entry anywhere.

The comment directly above said "Append, then act", which is the opposite of what the code did,
and that is the more interesting half. Append then act is impossible here. A pid and its start
token do not exist until the child does, so before the spawn there is nothing truthful to write
down. `aosd` had already worked this out: `Daemon::launch` appends, and if the append fails it
stops the process and says so. The cli had the same job and the wrong order.

Fix. On a failed append, stop the child before returning, and say in the error that it was
started, could not be recorded, and was stopped again. If the stop also fails, the message names
the pid, because at that point only a person can close the gap. The comment now describes the
real rule, which is that a start which could not be recorded is undone.

Guard. `a_start_that_cannot_be_recorded_leaves_no_surviving_child` runs a real
`/usr/bin/sleep` against a ledger whose every write fails, then scans `/proc` for anything whose
command line still carries the marker. `run` was split so the body is `supervise`, taking a
ledger and supervisor already built, because the failure being tested is a log that refuses
writes and `Ledger::open` insists on a file it can really open.

Three things went wrong while writing that guard, and each is worth keeping.

The marker started as a bare `4919`, and the pre check failed because an unrelated shell command
on this machine mentioned 4919 in its own arguments. `survivors` matches a substring of every
command line, so the marker has to be unique to this process. It now carries the test's own pid
after the decimal point, which `sleep` accepts because it takes a float.

The assertions were in the wrong order. The message check came first, so against the broken code
the test failed on the wording and never reached the claim about the surviving process. The
substantive claim now goes first, and the message check after it.

Worst of the three, the first version leaked. Against the broken code the test is the thing that
starts the process, so when it fails it is the thing that leaves it behind, which is bug 3 in
this list repeated in a new file. It now samples the survivors and sends SIGKILL to each before
asserting anything, so a failing run cleans up after itself. That is why `libc` is a dev
dependency of `aos-cli`.

Verified by restoring the bare `?` while keeping the seam and the test, and running it. It
failed with `left: [3545768], right: []`, naming the process that outlived the failed append,
and left nothing behind afterwards. The fix was then restored and the test passed, with
`cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and the full `cargo test` clean.

## bug 9, in full

The worst kind of hole: not a mistake inside a guard, but a whole entry point that never
reached one.

`aos start` goes to the daemon, and `Daemon::gate` decides before anything reaches the
supervisor. Its doc comment says nothing reaches the supervisor without passing through the
gate, and inside the daemon that is true. `aos run` is the other way to start an agent. It is
standalone, does not involve the daemon, and went straight from `load_spec` to `sup.start`.
`grep -rn "Policy\|Verdict\|PlanLedger" crates/aos-cli/src` returned nothing at all.

So the policy applied to one of the two ways to start an agent, on the same machine, with the
same allowlist and the same log. Which rules held came down to which subcommand somebody
typed. Reproduced before fixing: with every tier plus `hello` explicitly set to deny, `aos run`
started the child, exited 0, and wrote a `Started` record.

Fix. The deciding half of the gate moved into `aos-core` as `Gate`, returning a `Decision`
rather than a `Response`, and both entry points now go through it. Recording stayed with the
callers, because a refusal on a socket and a refusal on a terminal do not look the same and
the log each writes to belongs to the caller.

`Gate` has two entry points and the second one is the interesting part. `decide` is the full
handshake, for a caller that can hold a plan between two calls. `decide_without_handshake` is
for a caller that cannot, and `aos run` is exactly that: one process that starts an agent and
waits for it, so a plan it offered would die with it and there is no second call that could
quote one. Offering a plan there would be a lie, so anything above allow is refused and
pointed at the daemon. It takes `&self` rather than `&mut self`, which makes it impossible for
that path to leave a proposed plan behind.

The temptation was to give `aos run` a `--commit` flag for symmetry. That would have been
wrong. There is nothing to commit against, because the plan ledger it would have to consult
lives in whichever process issued the plan, and this process did not exist when that happened.

Guard. Three tests in `runtime.rs`, driving `run` against a real temporary run directory.
`a_denying_policy_refuses_aos_run` is the bug: a deny policy must produce an error and a
`Refused` record, and no `Started` record.
`a_prompt_tier_refuses_aos_run_and_points_at_the_daemon` checks the message names `aos start`
rather than silently doing nothing useful.
`an_allowed_agent_still_runs_to_completion` is the one that stops the fix being a denial of
service, and it checks the log reads exactly started then exited. Five more in `gate.rs` cover
the decision table itself, including that both entry points agree about read and that a per
agent deny beats an allowed tier on both.

Verified by disabling the new gate call and running the suite. The two refusal tests failed
and the allowed one still passed, which is the right shape: the guard fires on the hole and
not on ordinary use. Then the fix was restored and the shipped binary was pointed at the
issue's own repro, which now prints `policy denies hello at tier read`, exits 1, and writes a
`refused` record where it used to write a `started` one.

## bug 10, in full

The readme says `run/events.jsonl` is the only durable state. It was not durable.

`append` did `write_all` and then `flush`. `Write::flush` on a `std::fs::File` is a no op, and
not by accident: a `File` is a thin wrapper over a file descriptor with no userspace buffer,
so there is nothing to flush. The bytes went to the kernel page cache and the call returned.
Everything downstream then treated the record as written.

That is the one failure this whole design exists to prevent. `believed_running` is a fold over
the log, and the append first rule is there so the log can never claim less than actually
happened. A `Started` record sitting in the page cache when the machine loses power gives you
exactly the opposite: an agent genuinely running and no record that it was ever launched, so
nothing can find it, adopt it or stop it.

Fix. A `Durable` trait, which is `Write` plus `sync`, implemented for `File` as `sync_data`.
`Ledger` holds a `Box<dyn Durable>` and `append` syncs before it reports success, and before
`next_seq` moves, so a failed sync leaves the number unused rather than handing it to a record
the caller was never told about.

`sync_data` rather than `sync_all`, because the contents have to survive and the metadata does
not. A log with a stale mtime is still a log that reads correctly.

Making it a trait rather than a bare `self.file.sync_data()?` is the part that matters. "It
reached the disk" cannot be observed from a test, and this list does not take an entry without
one. Through the trait it can: the guard hands the ledger a sink that records what was done to
it and checks the tape.

Guard. `every_append_is_synced_after_it_is_written` appends two records over a counting sink
and asserts the tape reads `wsws`. Checking the ordering and not just the totals is deliberate,
because counting alone would pass against a version that wrote both records and synced twice at
the end, leaving the first one exposed in between.
`a_failed_sync_fails_the_append_and_does_not_burn_the_number` checks that a sink whose sync
fails makes the append fail, and that the sequence number is still available afterwards.

Verified by putting `flush` back where `sync` is. Both failed, the first with `left: "ww"`
against `right: "wsws"`, which is precisely the bug: written twice, synced never.

Cost. About 590 microseconds per append on ext4 against 9 without, over 200 appends. That is
65 times slower and it is still the right trade, because this log takes a handful of records
per agent rather than being a hot path. Written up in `infrastructure.md` under what durable
actually means here, along with what the log does and does not survive.

## bug 11, in full

One variant in the wrong arm of a match, and the kill switch got a survivor it could not see.

`believed_running` folds the log into the set of agents still running. `Event::Refused` sat in
the same arm as `Exited`, `Stopped` and `LostWhileUnsupervised`, all of which call
`live.remove`. A refusal is not any of those. It is a launch that never happened, so it cannot
have ended one that did.

The path to it is ordinary rather than exotic. Start an agent. Start it again. The supervisor
answers "worker is already running", the daemon records that refusal against the agent id,
which is right, and the fold then removed the live agent because of it.

After that the log believes nothing is running while the process carries on. `aos status`
reported nothing left running. A restarted daemon adopted nothing and wrote no
`LostWhileUnsupervised`, because from the log's point of view there was nothing to lose.
`stop-all` said "nothing was running". The one action the whole design promises can always be
taken could no longer reach it, and a second `aos start` is the most likely thing anybody
would type.

Fix. `Event::Refused` gets its own arm that does nothing, next to `Planned`. Both are records
of something that did not change the machine.

The other option was to skip writing the record when the agent is already running, and that
would have been the wrong half to give up. Refusals are the interesting half of an audit log:
a log holding only what happened cannot answer what somebody tried to do and was stopped from
doing. What the record means changes here, not whether it exists.

Guard. `a_refusal_does_not_erase_an_agent_that_is_already_running` is the bug: started then
refused, and the agent must still be live with its original handle.
`a_refusal_after_an_exit_leaves_the_agent_ended` is the other side, because a fix that made
`Refused` inert could have been written as one that revives an ended agent instead.

Worth noting why the existing tests missed it. `a_refusal_never_marks_an_agent_running` tests a
refusal with no start before it, and `every_ending_event_clears_the_agent` lists the three real
endings and, correctly, does not include `Refused`. Neither of them puts a start and a refusal
together, which is the only order in which this shows.

Verified by putting `Refused` back in the ending arm and watching the first test fail while the
second still passed. Then end to end against the real daemon: after the refused second start,
`aos status` prints `alive worker pid 3903717` and `stop-all` prints `stopped worker`, where
before both claimed there was nothing there.
