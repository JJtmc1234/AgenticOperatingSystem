# progress report

Where the plan stands. Updated 2026 08 07.

## summary

The project restarted on Linux in Rust. Phase 0 and all of phase 1 are complete and verified.
Phase 0r was added after reading Hunter's `agentic_os`. `aosd` now supervises agents that
outlive it, and `aos list` and `aos stop` work from any terminal. The Windows tree is archived
in the repo and removed from the working code.

| phase | target | status | note |
|---|---|---|---|
| 0 | 1 session | done | both acceptance criteria verified against real processes |
| 0r | 1 session | done | log replay and pid identity, verified against a real crash |
| 1a | 1 session | done | adoption, verified against genuinely orphaned processes |
| 1b | 1 session | done | the daemon, verified over a real socket |
| 2 to 6 | see planning.md | not started | policy and the commit handshake are next |

## what reading agentic_os changed

Hunter's kernel keeps its state as a fold over an append only log and replays that log on
boot. The comment on `emit` states the rule as append, fold, then broadcast, in that order,
always.

The log here was write only. That was a real gap rather than a stylistic one, because the
phase 1 daemon had no honest answer for what to do after a crash. It would have started blind
and either lost track of running agents or started duplicates.

So the audit log became the event log and the only durable state. `aos status` folds it and
reconciles the result against `/proc`. Two logs would have meant two versions of the truth, so
the old audit module was deleted rather than kept alongside.

A full comparison of what else transfers, and what does not, is in
[infrastructure.md](infrastructure.md).

## the part his design never has to solve

His agents are rows in a database. AOS agents are processes on a machine, and Linux reuses
pids. After a crash, the pid the log remembers may belong to something entirely unrelated, so
adopting it would mean signalling a stranger's process.

Every start record now carries the process start time from field 22 of `/proc/<pid>/stat`
alongside the pid. The kernel sets it once and never changes it, so the pair identifies a
process for as long as the machine has been up. Recovery adopts an agent only when both
match, and reports anything else as lost.

## why it restarted

New machine running Ubuntu 26.04. The Windows AOS had barely started, so the sunk cost was
small and the safety design was the only part worth keeping. [brainstorm.md](brainstorm.md)
has the full reasoning.

## what is verified working

Not compiled. Run.

| check | result |
|---|---|
| `cargo test` | 59 passing. Unit tests plus integration against real processes, real orphans and a real socket. |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt` | clean |
| `aos validate` on a good spec | reports the program and tier |
| `aos validate` on id `../etc` | refused, and the message names the bad id |
| `aos run` on an allowed program | started as a real pid, output captured, exit code reported |
| `aos run` on `/bin/sh` | refused, and the allowlist is named in the message |
| event log after both runs | two records, one started and one refused with a reason |
| kill switch | three sleeping agents, all three stopped, none left running |
| shell metacharacters in an argument | stayed literal, the injected `touch` never ran |
| supervisor killed with SIGKILL mid run | agent survived as an orphan, `aos status` reported it alive |
| the orphan then killed | `aos status` reported it lost, without being told |
| a live pid with a tampered start token | reported lost, and the running process was left alone |
| `/proc` stat parser against `we ) love ) parens` | still reads field 22 correctly |
| a genuinely orphaned process, reparented to init | adopted, reported running, stopped through its pidfd |
| the daemon killed with SIGTERM | its agent kept running, exactly as intended |
| the daemon restarted afterwards | re-adopted that agent once, marked adopted, not duplicated |
| `aos list` from a second terminal | showed the agent the first terminal started |
| a second daemon on the same run directory | refused, and the live one was left listening |
| the socket's permissions | 0600, asserted after bind rather than assumed |
| malformed requests, including a path shaped agent id | answered with an error, and the daemon stayed healthy |

The refusal record matters more than the success record. A log that only keeps what worked
hides exactly the calls worth reviewing.

The tampered token check is the one worth repeating. The process was genuinely alive and its
pid genuinely matched the log. Recovery still refused to claim it, which is the whole point.

## mistakes and their permanent guards

Full detail in [bug-list.md](bug-list.md).

| mistake | permanent guard |
|---|---|
| agent output piped with no reader, so output was lost and a chatty agent would deadlock | `a_chatty_agent_finishes_and_its_output_is_kept`, which pushes 1.3 MB through, plus `agent_output_lands_in_its_log` |
| a test helper waited on `setsid` with no bound, so a wrong assumption hung the suite for two minutes instead of failing | `wait_bounded` in `tests/adoption.rs`, which panics naming the cause after 5 seconds |
| the daemon test harness killed the daemon but not its agents, so a failing test left real processes on the machine | `Drop for Aosd` now sends `stop_all` before killing the daemon |

Bug 1 was found by running the binary rather than by a test. The suite was green while the
thing was broken, because every test used a program that printed almost nothing. Tests written
from the same assumption as the code do not catch the assumption being wrong.

Bug 2 is more embarrassing and more useful. The project already holds the rule that an
unbounded wait is a bug, in `signal::wait_bounded` and in the Windows AOS bug list before
that. The rule was written down and then broken in the next file. Writing a rule down is not
the same as following it, which is exactly why the guards have to be code.

Bug 3 got out of the test suite and onto the machine. Killing a daemon and leaving its agents
running is correct in production and a leak in a test, so the harness now stops the agents
first. A test that starts a real process owns it until that process is gone, especially when
the test fails.

Every guard was checked by reverting the fix and watching the test fail, which is the rule for
every entry in the bug list.

## deadlines

Nothing is late, because nothing has a hard date yet. Phase 1 was estimated at a week and came
in well under that, because phases 0r and 1a had already done its hard half. Phase 2, policy
and the plan then commit handshake, is next.

## what was learned

Rust's type system did real work here. `AgentId` cannot exist without passing its check, so
path traversal is refused once at construction rather than at every place an id becomes a
filename. That is a guard the compiler enforces rather than a rule someone has to remember.

Reading someone else's repo found a design gap that testing never would have. Every test was
green and every acceptance criterion was met, and the log was still write only. A test can
only check the thing you already thought of.

An identifier that the system reuses is not an identifier. That is true of pids and it will
be true of anything else AOS starts handing out numbers for.

Checking is not the same as holding. The start token proves a pid is yours at the moment you
look, and proves nothing at the moment you act. Closing that gap needed a different tool, a
pidfd, which is a handle the kernel guarantees will never point somewhere else. Worth
remembering as a shape: when a check and an action are separate steps, something has to hold
the answer still in between.

Linux gives away for free what Windows charged a kernel driver for. Process state is readable
under `/proc`, and resource limits are cgroups rather than a signed minifilter. Phase 4 is a
week here and was phase 7 and optional on Windows.

Running the thing is not optional. The pipe bug was invisible to a green test suite.

Order is a safety property. Append to the log, then act. A start that is recorded and then
fails leaves the log claiming one thing too many, which replay corrects against `/proc`. A
start that succeeds and is not recorded leaves a process nothing will ever know about. Only
one of those two mistakes is recoverable, so the code is arranged to only ever make that one.
