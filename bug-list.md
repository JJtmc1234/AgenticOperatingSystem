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
| 12 | The daemon reaped a finished child only as a side effect of a `list` or `ping`, and never appended `Event::Exited` at all. Children stayed zombies until somebody asked, and a clean exit was later reported as `lost_while_unsupervised`. | Never fired. Reproduced: an agent running `sleep 1` under `aosd` left a `Z [sleep] <defunct>` under the daemon, and the log kept only the `started` record. | `an_agent_that_finished_is_recorded_without_anyone_asking`, `a_running_agent_is_left_alone_by_the_reaper` |
| 13 | The shutdown flag was only checked between connections, and a session blocked in `read_line` until the peer hung up. One client that connected and said nothing made SIGTERM a no op, leaving SIGKILL as the only way to stop the daemon. | Reproduced. Connect, wait a second, send SIGTERM: the unfixed daemon is still alive after ten seconds, the fixed one exits at once. | The end to end check above, and `cargo test` for the rest. See the note on what is not unit tested. |
| 14 | `serve::run` booted the daemon before binding the socket, so a second `aosd` replayed the log and appended records before discovering a live daemon and exiting. It spent sequence numbers the live daemon believed were free, and every record that daemon wrote afterwards collided. | Reproduced: a second `aosd` printed "1 lost while unsupervised", appended a record, then failed with "a daemon is already listening", and the live daemon's next record reused the same number. | `a_second_writer_is_refused_rather_than_forking_the_sequence`, `reopening_a_damaged_log_never_goes_backwards`, `the_lock_is_released_when_the_ledger_is_dropped` |
| 15 | `Root::for_writing` tested `exists()`, which follows a symlink, so a link inside the root whose target did not exist yet was treated as an ordinary new file and the write followed it outside the root. | Never fired. Found by reading, then reproduced against the real server: `write_file` on a dangling link reported success while writing outside the root. | `a_dangling_symlink_pointing_outside_the_root_is_refused`, `a_dangling_symlink_pointing_inside_the_root_is_refused_too` |
| 16 | `Policy::load` treats a missing file as a request for the built in default, and `aos-files` used it for `--policy`, a required argument. A typo or a relative path silently replaced the operator's rules with more permissive ones, and the banner never named the policy. | Never fired. Reproduced: the real path denied `delete_file`, one changed character in the filename started fine and answered with a plan id. | `a_named_policy_that_is_missing_is_an_error_not_the_default`, `an_unnamed_missing_policy_is_still_the_safe_default`, `a_malformed_named_policy_is_still_an_error`, `the_summary_uses_the_words_the_policy_file_uses` |
| 17 | `read_file` read the whole file with `std::fs::read` and then cloned the buffer for the utf8 check, so peak memory was twice the file size. `MAX_READ` bounded the reply and not the read, so a large file inside the root took the process down with the OOM killer. | Never fired. Measured: reading an 80 MB file grew peak resident memory by 161,740 kB. | `reading_a_large_file_does_not_pull_it_all_into_memory` |
| 18 | `ledger::read` collected every line into one `Result`, so a single half written last line made the whole log unreadable and `Ledger::open` inherited that, so the daemon refused to boot at all. | Never fired. Reproduced: chopping 20 bytes off `events.jsonl` made `aosd` exit 1 with a parse error, leaving a live agent that nothing could adopt, stop or record as lost, on that boot or any later one. | `a_torn_final_line_is_dropped_rather_than_making_the_log_unreadable`, `a_torn_log_can_still_be_opened_and_appended_to`, `a_final_record_missing_only_its_newline_is_kept`, `a_corrupt_line_in_the_middle_is_still_an_error` |
| 19 | The allowlist was compared by string equality and the same string was handed to `Command::new`, so a bare name got a `$PATH` search and a relative one resolved against the daemon's working directory. The gate named one file and the kernel ran another. | Never fired. Reproduced: allowlist `["probetool"]` with the probe directory on `PATH` ran a binary the allowlist never named, and the ledger recorded only `"program":"probetool"`. | `a_bare_name_is_refused_at_load`, `a_relative_entry_with_a_slash_is_refused_at_load`, `a_different_file_with_the_same_name_is_refused`, `two_hard_linked_coreutils_stay_distinct_entries` |
| 20 | `start_token` collapsed every `/proc` read failure into `None`, and `None` meant "the process is gone". A comm field that is not UTF-8 makes `read_to_string` fail, so a live agent was written off, dropped from `believed_running` on every later boot, and put beyond `stop` and `stop-all`. An agent could escape the kill switch by renaming itself. | Reproduced on this machine with `prctl(PR_SET_NAME, "\xff\xfe-agent")`. | `a_name_that_is_not_utf8_does_not_hide_the_start_time`, `an_agent_that_cannot_be_checked_is_neither_adopted_nor_written_off`, `a_gone_agent_is_still_lost_rather_than_unknown` |
| 21 | Adoption trusted a pid and start token and nothing else. A token counts ticks since boot, so the pair means nothing across a reboot, and adoption never checked what the pid was running nor whether it is still allowed. A stale record could adopt a stranger, which `stop_all` then SIGKILLs. | Reproduced by pointing a log at a live process the supervisor never started: `alive` of 1, `is_adopted` true, and `stop` killed it. On this machine 87 processes share start token 18. | `a_handle_from_another_boot_is_lost_rather_than_matched`, `refuses_to_adopt_a_pid_running_a_different_program`, `refuses_to_adopt_a_program_the_allowlist_no_longer_permits` |
| 22 | `adopt_from` discarded the result of `adopt` with `let _ =`. An agent that recovery placed in `alive` but adoption then refused was in neither half of the outcome, so boot wrote no record, tracked nothing, and printed that it had adopted it. | Reproduced with one free descriptor and a real orphan: `alive [worker]`, `lost 0`, `is_adopted false`, `list` empty, boot printing "adopted 1 agent(s)", and no record written. | `an_adoption_that_fails_is_reported_rather_than_dropped` |
| 23 | `files::find` stopped at its limit and returned the hits with nothing saying the search was cut off, so a model that asked for every match and got the first 200 of 250 acts on those as if they were all. `limit: 0` answered "nothing matches" for a search that never read a directory. | Reproduced against the real server: 250 files, no limit given, exactly 200 lines with no mention of more, and `limit: 0` reporting no match for a file that exists. | `a_search_that_hit_the_limit_says_so`, `a_limit_hit_deep_in_the_tree_stops_everything`, `a_limit_of_zero_is_refused_rather_than_answered_with_nothing` |
| 24 | The `PlanRequired` arm of `aos start` printed the plan and returned `Ok(())`, so the process exited 0 having started nothing. A script could not tell "the gate stopped this" from "the agent is running". | Reproduced: `aos start examples/risky.json` printed "nothing has run" and exited 0, with only a `planned` record in the log. | The end to end check in the entry below, plus `cargo test` for the rest |
| 25 | The commit command `aos start` prints was built from the spec path and plan id only, so on any run directory other than the default it named the wrong daemon. The spec path was relative too, so it also failed from any other working directory. | Reproduced: copying the printed line verbatim gave "cannot read examples/risky.json" from `/tmp`. | The end to end check in the entry below |
| 26 | `aos run` supervised the agent in the foreground, printed its exit code, and returned `Ok(())`, so the process exited 0 whatever the agent did. The printed code was Rust `Debug` of an `Option`, so a reader looking for a number got the text `Some(1)`. | Reproduced with a spec running `/usr/bin/false`: `failer stopped, exit code Some(1)` and `EXIT=0`. | The end to end check in the entry below |
| 27 | `Supervisor::start` refused on the presence of a key rather than on the agent being alive. Nothing calls `try_wait` on a child except `state`, so an agent that finished by itself stayed in the map, and stayed a zombie, until somebody happened to run `aos list`. A legitimate restart was refused as "already running" about a process that had been dead for some time. | Read as issue 26. Reproduced by starting an agent that exits at once and starting it again with nothing asking the supervisor anything in between. | `an_agent_that_finished_on_its_own_can_be_started_again_with_nothing_asking_first` |
| 28 | `list` is built out of `state`, and `state` removes the agent it has just reported as stopped, so every stopped row named an id the map no longer held. The daemon then asked `is_adopted` about those ids and got false. The flag whose whole job is to explain a missing exit code was wrong in the one report that carries one. | Read as issue 27. Reproduced by adopting a real orphan, killing it from outside, and listing. | `a_stopped_adopted_agent_is_still_reported_as_adopted` |
| 29 | `find` is the only capability that does not resolve through the scope, so `refuse_secrets` was never consulted for it and the walk descended into `.ssh`, `.aws` and everything else on the secret list. `read_file` still refused those paths, so contents were safe, but the names and the tree shape came back in full. A search for `id_` answered with `.ssh/id_rsa`. | Read as issue 28. Reproduced against the real fixture, which already puts `.ssh/id_rsa` inside a read root. | `find_does_not_report_the_files_every_other_capability_refuses`, `find_still_reaches_ordinary_files_in_ordinary_directories` |
| 30 | `settles_an_order` treated every heartbeat as unsolicited, and `Ping` is the one order whose entire reply is a heartbeat. So one click on PING took the in flight slot and nothing ever gave it back, and `dispatch` refused every order after it. The whole command half of the panel was dead until a restart. | Read as issue 29. Reproduced by clicking PING once in `aos-panel` and then clicking anything else. | `a_heartbeat_settles_a_ping_because_that_is_the_whole_of_its_answer`, `a_ping_does_not_wedge_every_command_that_follows_it`, `a_heartbeat_does_not_settle_an_order_it_is_not_the_answer_to` |
| 31 | `nothing_the_panel_does_writes_to_the_ledger` called six commands in a row and only one of them reached the worker. `ping` took the in flight slot, `dispatch` dropped the next three at its guard, and `request_plan` and `commit` returned before reaching `dispatch` at all. The stated guard for the panel being read only against the ledger could not have failed for the reason it exists. | Read as issue 30, alongside bug 30 and separately from it. | The same test, rewritten to send one command at a time and assert each reached the worker |
| 32 | Four smaller things. `make_dir` told the agent it created any missing directories above the one asked for, and the resolver canonicalises the parent so only the last level can be missing. `StopMode` was defined and publicly re exported and nothing constructed, matched on or took one. `link_sentence` was written and never drawn, so the strip said NO DAEMON and withheld the line naming the fix. And nothing drove `move_file` through the server, so the rule that a move source must be writable was stated in a comment and guarded by nothing. | Read as issue 31. Each half of every pair read before it was reported. | `make_dir_creates_one_level_and_says_so_when_the_parent_is_missing`, `a_move_out_of_a_readable_directory_is_refused_at_the_source`, `a_move_inside_the_workspace_goes_through` |
| 33 | `aos run` appended into `events.jsonl`, the file `aosd` replays on boot and adopts from, while its own help said the daemon would not know about it. A daemon booting during a foreground run adopted an agent that already had an owner, and `aos stop` would then kill a process `aos run` was still waiting on. Once the ledger took an exclusive lock the same sharing stopped them working together at all. Separately, the append before act rule was written as an invariant with the word always, and both supervisors act then append. | Read as issue 32. The lock made the adoption race unreachable and turned it into a hard refusal instead, which is the shape the test checks. | `aos_run_leaves_the_daemons_log_alone_and_works_while_the_daemon_holds_it`, `status_reports_a_foreground_run_as_well_as_the_daemons_log` |
| 34 | The policy gate took the tier straight out of the caller's own request, so a caller picked its own tier and therefore its own verdict, and could skip the plan and commit handshake entirely. With `/usr/bin/rm` on the allowlist, two byte identical launches differing only in `"ceiling"` answered `plan_required` for destructive and `started` for read. The second deleted the target with no plan, no commit and no human, and the log recorded only `"event":"started"`. | Read as issue 15, then reproduced against a live `aosd`: `riska` at destructive answered plan_required and the directory survived, `riskb` at read answered started and it did not. | `a_spec_cannot_lower_its_own_tier_to_skip_the_handshake`, `a_ceiling_below_the_program_refuses_and_one_above_it_does_not_raise_anything`, `the_tier_comes_from_the_program`, `a_program_nobody_classified_is_not_treated_as_harmless`, `an_interpreter_or_a_privilege_raise_is_the_top_tier` |

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

## bug 12, in full

The daemon never wrote down that an agent finished. `Event::Exited` was produced in exactly one
place in the workspace, `aos-cli/src/runtime.rs`, which is the foreground `aos run` path. The
daemon, which is the thing that actually owns agents, wrote none.

Reaping was just as accidental. `Supervisor::state` removes an agent once it reports `Stopped`,
and the only callers were `list` and `ping`. So a finished child was reaped when a client
happened to ask and not before, and until then it sat as a zombie holding its process table
entry. Reproduced: an agent running `sleep 1` left `Z [sleep] <defunct>` under the daemon with
no request having arrived.

The consequence downstream is the one that matters. `believed_running` is a fold over the log,
and with no `exited` record the log kept calling the agent live. The next boot then found a pid
that was gone and wrote `lost_while_unsupervised`, which is meant to mean something went wrong,
for an agent that had exited cleanly with code 0. An alarm that fires on every normal exit is
an alarm nobody reads.

Fix. `Supervisor::reap_finished` reaps everything that has already stopped and reports each one
with its exit code. It reports rather than records, because the log belongs to the daemon and
the supervisor crate does not own it. `Daemon::record_exits` writes an `Exited` record for each,
and the accept loop calls it every time round, which is already every 20ms when idle.

One thing there is not clean and is worth naming rather than hiding. `record_exits` runs on a
timer, so there is no caller to hand an append failure back to. It goes to stderr, which under
systemd is journald. Stopping the daemon over it would be worse, and dropping it silently is
what bug 5 in the daemon was about, so saying so out loud is the least bad of three.

Guard. `an_agent_that_finished_is_recorded_without_anyone_asking` starts a real `/usr/bin/true`
through the daemon's own request path, then drives only the timer, never a client request, and
asserts the log reads `started` then `exited Some(0)` and that `believed_running` is empty
afterwards. That last assertion is the one tied to the real damage, since it is what stops the
next boot calling this a loss. `a_running_agent_is_left_alone_by_the_reaper` is the other half,
because a reaper that recorded exits for agents that had not exited would pass the first test.

Verified by making `record_exits` iterate an empty list instead. The first test failed and the
second still passed, which is the right shape. Then end to end against a real daemon: an agent
running `sleep 1`, three seconds of no client requests at all, no zombie under the daemon, and
the log holding both `started` and `exited` with code 0.

## bug 13, in full

The kill switch is the thing this design promises always works, and it could not stop the
supervisor.

`serve::run` checked the shutdown flag once per trip round the accept loop. Serving a
connection happens inside that trip, in `session`, which blocked in `read_line` until the peer
hung up. So while any client was connected, nothing looked at the flag. A client that connected
and then said nothing held the daemon open indefinitely, and SIGTERM did nothing at all.

The signal handler made it worse in a way that is easy to miss. glibc's `libc::signal` installs
a handler with `SA_RESTART`, so a read interrupted by a signal simply resumes. The flag was set
correctly and the blocked read went straight back to waiting, so even the interruption was
swallowed.

Fix, in four parts, because the failure had four contributing pieces and closing any one of
them alone still leaves a daemon that is slow or stuck to shut down.

A read timeout on the accepted stream, so `read_line` comes back to the loop.

The shutdown flag checked inside the session loop rather than only between connections.

`sigaction` with no `SA_RESTART` instead of `signal`, so a read that is blocked when the signal
lands returns `EINTR` at once rather than waiting out the timeout as well.

A read timeout in `client::ask`, so a wedged daemon surfaces as an error rather than a terminal
that never comes back. Thirty seconds, which is generous because the daemon serves one
connection at a time on purpose, so a slow neighbour is a real reason to wait.

One detail in the session loop is load bearing and easy to undo. The `String` being read into
lives outside the loop and is deliberately not cleared on a timeout. `read_line` may have taken
part of a line before the timeout expired, and starting a fresh buffer would split the request
in half. That is why this is a manual `read_line` loop rather than `for line in reader.lines()`,
which drops what it had on an error.

Verified end to end, both ways, with identical timing: connect, wait one second so the daemon is
genuinely inside `session`, send SIGTERM. The unfixed daemon is still alive after ten seconds.
The fixed one exits immediately and prints "aosd stopped, agents left running".

The one second wait matters, and getting it wrong the first time is worth recording. Without it
the signal arrives while the daemon is still in the accept loop, which was never broken, so both
versions exit at once and the test says nothing. A reproduction that does not reproduce is not
evidence, and it looked like a pass.

Not unit tested, and that is a real limit. This is the interaction of a signal, a blocking read
and a process lifetime, and the existing daemon tests drive the real binary over a real socket
for exactly that reason. The check above belongs with them and is written down here instead.

## bug 14, in full

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

## bug 15, in full

The root check had two paths and only one of them could see what it was looking at.

`existing` canonicalizes and then checks the result is inside the root, which resolves any
symlink on the way and is correct. `for_writing` cannot do that, because the file being
created does not exist yet and an unresolvable path cannot be canonicalized. So it asks
whether the path is already there, and if not it checks the parent instead.

The question it asked was `joined.exists()`. `exists` follows symlinks, so for a link it
answers about the target rather than about the name. A link inside the root pointing at
something that does not exist yet therefore answered false. That took the parent branch, the
parent is the root, the root is inside itself, and `for_writing` handed back
`<root>/<linkname>`. `std::fs::write` then followed the link and created the file wherever it
pointed.

The existing symlink test did not catch it because it links to a directory that already
exists. `exists()` is true there, the checked path is taken, and it passes. The dangling case
is the one nobody wrote down.

Real shapes this takes are ordinary rather than exotic. A checkout carrying
`config -> ../../.ssh/authorized_keys`. A stale `latest -> builds/2026-08-18/out.log` left
behind after the build directory was cleaned. Neither looks like an attack and both are enough.

Fix. `std::fs::symlink_metadata(&joined).is_ok()` rather than `joined.exists()`.
`symlink_metadata` does not follow the link, so it answers about the name, which is the
question actually being asked. A dangling link now takes the `existing` branch, where
`canonicalize` fails and the call is refused.

That refuses a dangling link pointing inside the root as well, and that narrowing is
deliberate rather than collateral. The target cannot be canonicalized, so it cannot be proven
to be inside the root, and this component does not guess. Refusing something harmless is the
price of never allowing something that is not. There is a second test saying so, on purpose,
so nobody later reads it as an oversight and loosens it.

Guard. `a_dangling_symlink_pointing_outside_the_root_is_refused` builds the exact shape, checks
the call is refused, and checks the target still does not exist afterwards, because a fix that
refuses while still creating the file would pass a weaker assertion.
`a_dangling_symlink_pointing_inside_the_root_is_refused_too` pins the narrowing.

Verified by writing both tests first and watching them fail against the unfixed code, then
applying the one line change and watching them pass. Then end to end against the real binary:
`write_file` on a dangling link into a directory outside the root now answers
`refused: innocent: No such file or directory` and the target is still not there afterwards.
Before the fix the same call reported writing 5 bytes and the file appeared outside the root.

## bug 16, in full

One loader used for two questions that only look the same.

"There is no policy file, so use the default" is correct for a run directory. It is documented
in the readme, it is why `Policy::default` exists, and the default is deliberately strict:
read runs, everything else needs a human.

"The policy file you named is not there" is a different question with a different answer.
`aos-files` takes `--policy` as a required argument, so naming a file is a statement that its
rules are the ones to enforce. It called `Policy::load`, which answered the first question, so
a path that did not resolve produced the built in default and the server started happily.

The default is more permissive than most policies anybody bothers to write. An operator who
wrote `destructive = "deny"` got `prompt` instead, which a model can satisfy on its own with a
plan round trip. So the failure mode is a quiet loosening, from a typo, with nothing said.

Worse than a typo: Claude Code starts an MCP server with its own working directory, not the
one the operator was standing in. A relative `--policy run/policy.toml` therefore misses
whenever the client was started from anywhere but the repo root, which is most of the time.

Fix. `Policy::load_required` errors on a missing file. `aos-files` uses it. `Policy::load`
keeps the defaulting behaviour, because the run directory case is real and wanted, and there
is a test pinning that so this fix does not get generalised into it later.

The banner is the other half and it is not decoration. The failure was invisible: a server
enforcing rules nobody asked for while looking perfectly healthy. It now prints the resolved
policy path and the verdict per tier, so what is in force can be read at a glance:

```
aos-files: policy /path/to/policy.toml [read=allow write=prompt system=prompt destructive=deny]
```

`Verdict` grew a `Display` that spells each one the way the policy file spells it, so the
banner is not a second vocabulary to learn.

Guard. Four tests. The first asserts both halves in one place, that `load` still defaults and
`load_required` does not, so the distinction cannot be collapsed by accident.
`an_unnamed_missing_policy_is_still_the_safe_default` pins the behaviour that was correct all
along.

Verified against the real binary, both halves of the reproduction. With the correct path the
server starts and prints `destructive=deny`. With one character changed in the filename it
exits 1 saying the policy could not be read and why a named policy is not optional, where
before it started and enforced the default.

## bug 17, in full

`MAX_READ` is 200 kB and it was applied to the wrong thing.

`read_file` called `std::fs::read`, which allocates the whole file, then `String::from_utf8`
on `bytes.clone()`, a second full copy, because `bytes` had to stay alive for the lossy branch.
Only then was `MAX_READ` applied, to the decoded string. So the cap bounded the reply and did
nothing at all about the read. Peak memory was twice the size of whatever file was named.

The root is a directory an agent may name anything inside. A multi gigabyte log, a database
dump or a core file sitting in there needs roughly twice its size before a single byte is
truncated, and the OOM killer takes the process down. That drops the whole MCP session, not
just the one call, so every other capability goes with it.

Measured rather than argued: reading an 80 MB file grew peak resident memory by 161,740 kB.

Fix. Stat for the size, then read at most `READ_CAP` bytes through `Read::take`, so the read
is bounded by the same order as the reply. The clone is gone as well: matching on
`std::str::from_utf8(&bytes)` borrows, and the lossy branch borrows the same buffer.

`READ_CAP` is `MAX_READ + 4`, and the four bytes are load bearing. They are one utf8 character,
which is what makes it possible to tell a cut this code made from a file that is genuinely not
utf8. `Utf8Error::error_len` is `None` only for input that ran out part way through a
character. If the read stopped early then that is where the character went, so the valid prefix
is used and nothing is said about the file. Reporting it as mangled would have an agent
reasoning about damage that is not there, which is the exact failure the note exists to
prevent.

The truncation note now reports the size from the metadata rather than the length of the text,
because the text is no longer the whole file and the note would otherwise always claim the file
is exactly as long as the cap.

Guard, and it is worth being exact about what each test covers, because only one of the four
guards the bug itself.

`reading_a_large_file_does_not_pull_it_all_into_memory` is the bug. It writes 80 MB, reads it,
and compares `VmHWM` from `/proc/self/status` before and after. It asserts on what the process
did to the machine rather than on how the function is written, which is the property that
actually matters. Against the previous code it fails with the 161,740 kB figure above.

The other three guard hazards this fix introduces rather than the one it removes, and they pass
against the old code as well. That is not a weakness in them, it is what they are for.
`a_cut_through_a_character_is_not_reported_as_a_broken_file` and
`the_truncation_note_gives_the_real_file_size` both describe things that could not go wrong
while the whole file was being read, and can now. `a_genuinely_invalid_file_is_still_reported`
pins the behaviour the first of those must not have swallowed.

## bug 18, in full

The log is the only durable state, and a single interrupted write made it unreadable in full.

`read` ended in `.collect()` into a `Result<Vec<Record>>`, which fails the whole collection if
any line fails. `Ledger::open` calls `read` to find the next sequence number, so the daemon
inherited it and refused to start. Reproduced by starting an agent, killing the daemon while
the agent kept running, and chopping 20 bytes off the end of `events.jsonl`: `aosd` exited 1
with `EOF while parsing a string`.

The consequence is worse than a failed boot. The agents from the previous run are still on the
machine. Refusing to boot means nothing adopts them, nothing stops them and nothing records
them as lost, and that is true of every later boot as well, because the file does not heal.
The kill switch cannot reach them either. It needs a human editing the log by hand.

Fix, in two halves, because reading and appending fail differently.

`read` now tolerates an unparsable line only when it is both the last line and the file does
not end in a newline. All three conditions, or it is real corruption. A line that was
completed once and is now unreadable means the log disagrees with what happened, and skipping
that quietly is exactly the thing this project refuses to do.

`Ledger::open` repairs the tail before reading, because tolerating a torn line on read is not
enough on its own: the next append would be written onto the end of it, and one line holding
half a record followed by a whole one parses as neither, so the good record would be lost too.

The repair has two cases and the second is the one worth writing down. If the trailing bytes
do not parse, the write was interrupted part way through, there is nothing there to keep, and
it is truncated away. But if they do parse, the record itself was fully written and only its
newline was not, so it is a complete record and the newline is added instead. Truncating back
to the last newline unconditionally is the obvious repair and it would throw that record away.
`a_final_record_missing_only_its_newline_is_kept` is the test that says so.

What this does not do, and it is worth being exact. It does not recover the torn record. In
the reproduction above the torn line was the only `started` record, so after the fix the
daemon boots and reports no agents while the sleeper is still running. The fix makes the log
readable and appendable again. It cannot invent a record that was never finished. Bug 5, the
missing `sync_data`, is the one that narrows how often a record is lost at all, and the two
are separate: an `ENOSPC` part way through `write_all` produces this same file whatever the
sync policy is.

Guard. Four tests. Three of them fail against the previous code and the fourth,
`a_corrupt_line_in_the_middle_is_still_an_error`, passes both before and after, which is the
point of it: it pins the behaviour that must not be loosened while making the tail forgiving.

Verified by putting the old `.collect()` back and removing the repair call. The three failed
and the fourth passed. Then end to end against the real binary: the daemon that used to exit 1
over a torn log now starts, listens and answers `list` and `stop-all`.

## bug 19, in full

The allowlist is the gate that decides which binary may start. It was comparing spellings.

`spawn::launch` tested `allowed.iter().any(|p| p == &spec.program)`, a string equality, and
then passed that same string to `Command::new`. Nothing in between touched the filesystem, so
nothing ever established that the string named a particular file. Two ways that goes wrong,
and both were reproduced rather than reasoned about.

A name with no slash makes `Command::new` search `$PATH` at exec time. Allowlist
`["probetool"]`, the probe directory prepended to `PATH`, and the agent ran
`<scratch>/lab/bin/probetool`, a file the allowlist never named. A relative name with a slash
resolves against the process working directory instead, so the same allowlist file named
different binaries depending on where `aosd` happened to be started from.

The audit side was just as bad. `Event::Started` recorded `spec.program.clone()`, the
unresolved spelling, so `events.jsonl` could not say which file ran even afterwards.

Fix. `aos_core::Allowlist` resolves every entry once at load, refusing any that is not
absolute and any that does not exist. `resolve_program` canonicalizes the requested program
and compares real paths. The resolved path is what gets spawned and what gets recorded.

Refusing a relative entry at load rather than at launch is deliberate. A relative entry is not
a narrower permission, it is an ambiguous one, and an ambiguous rule should be rejected before
anything has been decided by it rather than at the moment it was supposed to govern something.

The interesting part is what this machine taught about the alternative. The issue suggested
comparing device and inode instead, to also catch hard links. That would have been a disaster
here. `/usr/bin/echo` and `/usr/bin/sleep` canonicalize to
`/usr/lib/cargo/bin/coreutils/echo` and `.../sleep`, which are two names for **one inode**,
because uutils ships a single multi-call binary hard linked under every utility name. Under an
inode comparison, allowing `echo` would have allowed `sleep`, `rm` and everything else in that
binary, while the allowlist file still looked exactly as restrictive as before. Comparing
canonical paths keeps them distinct. The cost is that a hard link to an allowed binary under
another path is refused, which is the right way round.

That is checked, not just written down. `two_hard_linked_coreutils_stay_distinct_entries`
confirms the two really do share an inode on this host and that an entry naming only `echo`
still refuses `sleep`. It returns early rather than failing where the layout differs, since it
is asserting something about the machine rather than about this crate.

One more thing follows from multi-call binaries and is easy to undo by accident. The caller
spawns the resolved path, so `argv[0]` is the file that was actually checked and the binary
behaves as that. Passing the requested spelling as `arg0` would look like a tidy way to
preserve behaviour and would reopen the hole: a symlink named `rm` pointing at an allowed
`echo` would pass the check and then behave as `rm`. There is a comment on
`resolve_program` saying so.

Guard. Seven tests on `Allowlist`, four of them named in the table above. Verified against the
real binary both ways: with allowlist `["probetool"]` and the probe directory on `PATH`,
`aos run` now refuses at load with a message naming `$PATH`, and no agent log is created
because nothing ran. With allowlist `["/usr/bin/echo"]` and a request naming `/bin/echo`, it
runs and the ledger records `/usr/lib/cargo/bin/coreutils/echo`, the file that actually ran,
where before it would have recorded `/bin/echo`.

## bug 20, in full

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


## bug 21, in full

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


## bug 22, in full

A comment that was true about one cause, applied to all of them.

`adopt_from` ended with `let _ = self.adopt(...)`, and the comment above it said a failure there
means the agent died between recovery and adoption, so staying untracked is safe. That is
correct for that one cause and it is the only harmless one.

Every other cause leaves a live agent that the supervisor is not tracking while boot reports it
adopted and writes no record at all. `aos list` cannot see it. `stop-all` cannot reach it. And
because the id is now free, a later `aos start` under the same name appends a second `Started`
that supersedes the first in `believed_running`, so no later boot ever looks at the original
process again. It is running, unsupervised, and unreferenced.

The causes are not exotic. A kernel or a container seccomp profile without `pidfd_open` fails
every adoption on every boot. Descriptor pressure does it too, and in a way worth knowing about:
`PidFd::open` holds a descriptor across its second `is_still` check, so one free descriptor is
enough for the open and not enough for the recheck.

Fix. The result is kept. A refused adoption goes into `unknown`, next to the agents `/proc`
would not answer about and the ones running the wrong program, all of which mean the same thing:
something is on that pid, this supervisor will not touch it, and a person needs to look.

Into `unknown` and not into `lost`, deliberately, and the issue was explicit about why. Adoption
failing does not say the agent is gone. It says this supervisor could not take it back. Writing
a `lost_while_unsupervised` record would state that it ended, which is a claim nothing has
established and which `believed_running` would then act on forever.

This is the third bug in this file from the same shape, after the ledger appends and the `/proc`
read: a `Result` or an `Option` carrying an answer that was thrown away, with a comment
explaining why one of its cases was harmless.

Guard. The failure is forced by adopting the id first, so the second adoption is refused for a
reason that has nothing to do with the process being gone, which is the whole point. It asserts
all three: not in `alive`, present in `unknown`, and not in `lost`. Verified by putting
`let _ =` back, where it fails on the first of those.

## bug 23, in full

Two of the three read capabilities say when they cut. The third did not.

`list_dir` appends "... and N more". `read_file` appends "... truncated". `find` walked until it
had `limit` hits and then returned them, with nothing anywhere saying the search had stopped
early. A model that asked for every match and got the first 200 of 250 has no way to tell, and
will act on those 200 as if they were all of them.

The order makes it worse. The hits come back in filesystem walk order, so "the first 200" means
the first 200 the kernel happened to hand over, not the newest or the closest or anything the
caller chose. Reproduced against the real server with 250 files: the first two lines were
`report250.txt` and `report249.txt`.

Fix. `walk` returns whether it saw the whole tree, and `find` appends a line naming the limit
and saying the order is not the caller's, so the reader knows both that there is more and that
what they have is not a meaningful selection.

One thing that needed care. The limit can be reached inside a subdirectory, and returning `true`
from that branch let the parent carry on walking. That collects past the limit and, worse,
reports the search as complete, which is the original bug wearing a different hat. The recursive
call now stops the whole walk. There is a test that fills a deep directory and puts a match at
the top, which fails if only the branch stops.

The second half is `limit: 0`. The schema declares it as a plain integer with no minimum and the
handler clamped only the top, so `walk` returned before reading a single directory and `find`
took the empty branch and said "nothing under the root matches", for a file that was sitting
right there. That is a confident wrong answer rather than an empty one.

Refused rather than clamped up to 1. A caller who asked for no results has said something
contradictory, and quietly giving them one result is a different answer from the one they asked
for. Refusing says what happened.

Verified against the real binary. 250 files, no limit given: 200 hits plus the marker, and the
marker says so. `limit: 0` now refuses instead of denying a file that exists.

## bug 24, in full

The one outcome a caller most needs to branch on was the one it could not see.

`aos start` on a spec above tier read prints the plan and stops, which is the whole point of the
handshake. It then returned `Ok(())`, so the process exited 0. To anything reading exit status,
"the gate refused and nothing ran" was indistinguishable from "the agent is now running".

`aos start x.json && echo up` prints `up`. A wrapper doing `aos start x.json || exit 1` carries
on believing the agent is supervised, when the daemon deliberately declined to act. That is the
one case where the machine did not change, so it is the case a script most needs to know about.

Fix. An `Exit` enum in `main`: 0 acted, 1 failed, 2 refused pending agreement.

Two rather than one, deliberately. A script has to tell "needs a commit", which it can act on by
committing, from "the daemon is down", which it cannot. Collapsing them into a single nonzero
would replace one indistinguishable pair with another.

`stop_all` keeps its existing `bail` on a partial stop, which is exit 1, and that is not an
oversight. Some agents did not stop, which is worse than nothing having happened: the machine is
not quiet and no amount of agreeing fixes it. The issue noted that arm as precedent for treating
a partial no op as a failure, and it is, but not as the same kind of failure.

`main` returns `ExitCode` rather than `Result`, so the error path prints in the same shape
`anyhow` used to produce, since that is what people are used to reading.

Verified against a live daemon. `aos start examples/risky.json` prints "tier destructive needs a
commit, so nothing has run." and exits **2**, with only a `planned` record in the log. A read
tier spec still exits 0 and starts.

## bug 25, in full

The remedy the tool hands you, for the one flow with a deadline on it, did not work when copied.

Above tier read, `aos start` prints a line to rerun with `--commit`. It was built from the spec
path and the plan id. `--run-dir` is global and defaults to `run`, so on any other run directory
the printed command pointed at the wrong daemon. The default plan ttl is 120 seconds, so the
round trip spent working out what was missing can expire the plan and force the whole handshake
again.

The run directory is what the issue asked for, and adding it was not enough, which is the part
worth keeping.

The issue's justification was that the line should be copy and paste safe. With the run
directory added it still was not:

```
  aos --run-dir /tmp/aos-24 start examples/risky.json --commit 66d4ab71...
```

Run that from anywhere but the directory it was printed in and it fails with "cannot read
examples/risky.json", because the spec path is relative too. Fixing one and not the other leaves
the same failure reachable by walking to a different directory instead of forgetting a flag.

So both paths are resolved. `absolute` canonicalizes and falls back to the path as given, which
is the right answer for something that will not resolve: a line that is still wrong beats one
that is wrong and pretends otherwise.

Always rather than only when the value differs from the default, which is what the issue asked
for and is right. A line that is sometimes complete is a line nobody can trust without reading
it, and the conditional is one more thing to get wrong.

Verified by copying the printed line verbatim and running it from `/tmp`. Before: "cannot read
examples/risky.json". After: "risky started as pid 261291".

## bug 26, in full

A foreground supervisor that did not hand back what it supervised.

`aos run` is the standalone path, which is exactly the shape somebody wraps in a script or a
systemd unit, and every other foreground supervisor propagates the child's status. This one
printed the code and returned `Ok(())`, so the process exited 0 whatever the agent did. A cron
job doing `aos run brief.json || alert` never fires the alert.

The printed line had its own problem. `{code:?}` is Rust `Debug` of an `Option`, so somebody
looking for the number read the literal text `Some(1)`, and a script trying to recover it had to
parse it back out of debug syntax. The same shape was in `commands::list`, and both are fixed.

Fix. `Exit` grows an `Agent(u8)` variant carrying the child's status, and `run` returns it.
Carried through rather than flattened into the existing codes, because the whole point is that
the number is the agent's rather than this tool's.

A signal death becomes 128, and the reasoning is worth writing down because the obvious choice
is 128 plus the signal number. That number is not available here: `AgentState::Stopped` carries
an `Option<i32>` which is already `None` by the time it arrives, so the signal is gone before
this code sees it. 128 says a signal ended it and does not pretend to say which. Recording the
signal number would mean changing the event format, which is a larger change than this issue.

`describe` replaces the debug printing: "exit code 1", or "ended by a signal" where there is no
number, which beats printing `None` and leaving somebody to work out what that meant.

Verified end to end. `/usr/bin/false` gives "exit code 1" and `EXIT=1`. `/usr/bin/true` gives
"exit code 0" and `EXIT=0`. An agent killed with SIGTERM mid run gives "ended by a signal" and
`EXIT=128`.

## bugs 27 and 28, in full

Two ways the supervisor answered a question about an agent from a map rather than from the
agent.

Bug 27 is the start path. The refusal message said "is already running" and the check behind it
was `self.agents.contains_key`. Those are not the same question. The only thing that ever takes
a key out for an agent that exited on its own is `state`, which is reached from `list`, from
`ping` and from the daemon's exit tick, and none of those are on the start path. So an agent
that ran for a second and finished held its own id hostage, and the message a person got said
the opposite of the truth.

Fix. `start` asks `state` when it finds a key. Stopped means the entry has just been reaped and
the id is free. Running refuses and now names the pid, so the claim can be checked. An error
refuses too, for the same reason an unreadable `/proc` is not treated as a dead agent: going on
would overwrite the entry for a child that may well still be alive.

Bug 28 is the reporting path. `list` mutates the map it is describing, so the flag that explains
a missing exit code was answered after the agent it describes had been forgotten. `list` now
returns a `Listed` carrying the flag it read before reaping, and the daemon builds its report
from that rather than asking a second question.

The test for bug 27 cannot use the existing `wait_for_exit` helper, because that helper calls
`state` and `state` is what reaps. It waits on the zombie in `/proc` instead, which is a state
the process holds rather than a moment the test has to catch.

## bug 29, in full

The one capability that did not go through the scope.

Every other file capability resolves through `Scope::to_read`, `to_change` or `to_remove`, and
those three are the only callers of `refuse_secrets`. `find` resolves to `Resolved::Whole` and
is handed the read root directly, so nothing in the secret list was ever consulted for it.

The contents were never at risk. `read_file` still refuses every one of those paths. What leaked
was the existence, the names and the layout of the files the server was built to refuse, which
is most of what somebody wants before they go looking for a way in.

Fix. The name check is split out of `secret_part` as `is_secret_name`, and the walk skips a
matching entry outright rather than merely leaving it out of the hits. Outright matters: a
refused directory must not be descended into, or the rule holds for the directory and not for
anything inside it.

The comment above `SECRET_NAMES` says these are names refused wherever they appear. That is now
true.

## bugs 30 and 31, in full

A wedged panel, and the test that should have caught the wedge but ran one sixth of itself.

Bug 30. `dispatch` is the single gate for every order and it refuses to send while one is
outstanding. The slot comes back when an outcome settles, and settling was defined as anything
that is not a heartbeat. `Ping` is the one order whose entire reply is a heartbeat. So a ping a
person asked for was sent, answered, and never settled, and from that moment PING, every STOP,
STOP EVERY AGENT, READ SPEC, FORGET IT, COMMIT THIS PLAN and DISCARD all did nothing.

The guard the old definition was protecting is real and still holds. The worker pings on its own
every two seconds, and one of those must never make an outstanding stop look finished. What was
missing is that which order is outstanding is part of the question, so `settles` now takes it.

Bug 31 is the test next door. `nothing_the_panel_does_writes_to_the_ledger` says in its own
comment that it sends every command a person can reach. It called `ping` first, which took the
slot, so `stop`, `stop_all` and `load_spec` were dropped at the guard and `request_plan` and
`commit` returned before reaching `dispatch` at all, the second pair because the load that would
have advanced the state was itself one of the dropped calls. The single `tick` that could have
delivered a reply was after all six.

The assertions then compared ledger bytes, modification time and directory listing, all of which
are trivially unchanged when nothing was sent. A guard that cannot fail for the reason it exists
reads as evidence that the property holds, which is worse than no guard.

Fix. One command at a time, each one waited for, and each one asserted to have reached the worker
before the next goes out. `commit` needs a state only a daemon can put the panel into, so the
test puts it there by hand and says why. Fixing bug 30 does not fix this on its own: even with
`Ping` settling correctly, the reply could not arrive until the tick at the end.

## bug 32, in full

Four small things, each one a sentence and its code disagreeing.

`make_dir` said "Create a directory, and any missing directories above it." `files::make_dir`
would honour that, since it calls `create_dir_all`, but the call never gets there.
`Root::for_writing` canonicalises the parent, which is how a path that does not exist yet is
checked for being inside the root at all, so `make_dir a/b/c` with `a` absent is refused. Either
end could have moved. The sentence moved, because walking up instead would weaken the
containment check for the sake of one convenience.

`StopMode` named `Graceful` and `Forced`. The escalation it describes is real and lives in
`stop_child` and `stop_pinned`, which send SIGTERM, wait the grace, then send SIGKILL. But the
mode is not a choice a caller makes: every stop does both, in that order. Nothing constructed a
variant, matched on one, or named the type in a signature. A type that names something real
and is not the thing enforcing it reads as a knob that exists, so it is gone and the reasoning
is in the module instead.

`link_sentence` was written, documented, and never called. `link_badge` directly above it is
drawn, so the strip showed the word for the link state and withheld the sentence explaining it.
On a machine with no daemon running that sentence is the one that names the fix. It is drawn
now, on its own line, which is what its own doc comment said it needed.

And `move_file` had no end to end test. The server resolves the source with `to_remove` rather
than `to_read`, with a comment saying exactly why, and `crates/aos-mcp/tests/server.rs` did not
contain the string `move_file` once. Swapping that one call would have let an agent empty a
directory it was given to read, and every test in the suite would still have passed. Checked by
making that swap: the new test fails, and the file really is moved out.

## bug 33, in full

Who owns the log, and what the log rule actually is.

`aos run` supervises an agent in the foreground and owns it for as long as it lives. It wrote
its records into `run/events.jsonl`, which is the file `aosd` replays on boot and adopts
everything from. So a daemon started while a foreground run was going took ownership of an
agent that already had an owner. Both then believed they held it, and a stop through the daemon
would kill a process the foreground run was still waiting on. The help text for the command
says, in as many words, that the daemon will not know about it.

The exclusive lock from bug 11 had already turned the race into something blunter. A second
opener of the same log is refused, so `aos run` against a run directory with a live daemon could
not open the log at all, and the two subcommands could not both be used. That is the shape the
test checks, because it can be checked without racing two processes.

Fix. `aos run` writes to `run/foreground/<agent>.jsonl`. One file per agent, so two runs of
different agents do not queue behind each other while two of the same agent still collide, which
is a real conflict and should be refused. `aos status` reads the daemon log and every foreground
log and reports them under separate headings, because separate files must not mean an agent
nothing reports. They are not merged: each file numbers its own records, and merging two would
put them in an order neither file claims.

The second half is a sentence. `event.rs` said a record is written before the belief changes,
never after, borrowed the shape from Hunter's kernel, and ended with the word always.
`infrastructure.md` said the same. Neither supervisor does that, and neither can: a `started`
record carries a pid and a start token that do not exist until the child does, and an `exited`
record carries a code that does not exist until the process is over.

The behaviour is right and the sentence was wrong, so the sentence changed. The rule those two
paths obey is that a change which could not be recorded is either undone or reported as
unrecorded, never dropped. `Daemon::launch` stops the process it could not write down and so
does `aos run`. A stop that could not be written is reported as stopped and unrecorded, because
undoing a stop is not possible and pretending it failed would be false. Everything that can
append first still does.

## bug 34, in full

The gate asked the caller how dangerous the caller was.

`Daemon::gate` read `let tier = spec.ceiling` and handed that to `policy.verdict`. `ceiling`
arrives verbatim in the `Start` request, so the number the gate judged by was chosen by the
thing being judged. That is not a gate. Sending the same launch twice with nothing different
but that one string gave `plan_required` for `destructive` and `started` for `read`, and only
the second one deleted anything. The log then recorded a plain `started`, so nothing afterwards
could tell that a destructive action had run.

Fix, in two halves.

The tier now comes from the program, through a table in `aos-core/src/program.rs` keyed on the
file name so `/bin/rm` and `/usr/bin/rm` are the same program. A program nobody has classified
answers System rather than Read, because Read means changes nothing and nothing here has
established that about a program it has never heard of. System rather than Destructive, because
the top tier should mean this particular thing loses data that cannot come back, and flattening
the two would make it stop meaning anything. Under the default policy System prompts, so an
unknown program gets a plan and a person rather than a refusal or a free pass.

Interpreters and shells are at the top tier. Each takes code on its own argument vector, so
allowing one grants everything every other gate protects, which is why CLAUDE.md says never to
put one on an allowlist. If one ever gets there anyway, it needs a human every time instead of
inheriting whatever tier the caller felt like claiming.

Arguments are deliberately not consulted. A classifier over argument strings looks like a gate
and is not one: alternative spellings, quoting, `--`, long and short flags and an environment
prefix all reach the same effect under a different string, and every near miss reads as allowed.
A tier is a property of what the binary can do, and the binary is what a human wrote on the
allowlist.

The second half is `ceiling`, which now does the job its own doc comment always described. It is
the highest tier this agent may reach, so a spec whose program is above it is refused. A spec
can lower what it is allowed to do and can never raise it, and a ceiling above the program buys
nothing, since the tier is still the program's.

Every test that reached the prompting path by writing `"ceiling":"destructive"` next to
`/usr/bin/sleep` was proving nothing, and each one now names a program that really is at the
tier it tests. `examples/risky.json` was the same fiction and now runs `rm -f` on a scratch path.
The destructive programs in the test suite are pointed at paths carrying the test process id
that cannot exist, so `-f` exits 0 having deleted nothing.
