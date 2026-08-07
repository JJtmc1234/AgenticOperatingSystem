# progress report

Where the plan stands. Updated 2026 08 07.

## summary

The project restarted on Linux in Rust. Phase 0 is complete and verified. The Windows tree is
archived in the repo and removed from the working code.

| phase | target | status | note |
|---|---|---|---|
| 0 | 1 session | done | both acceptance criteria verified against real processes |
| 1 to 6 | see planning.md | not started | |

## why it restarted

New machine running Ubuntu 26.04. The Windows AOS had barely started, so the sunk cost was
small and the safety design was the only part worth keeping. [brainstorm.md](brainstorm.md)
has the full reasoning.

## what is verified working

Not compiled. Run.

| check | result |
|---|---|
| `cargo test` | 18 passing. 9 unit, 9 integration against real child processes. |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `cargo fmt` | clean |
| `aos validate` on a good spec | reports the program and tier |
| `aos validate` on id `../etc` | refused, and the message names the bad id |
| `aos run` on an allowed program | started as a real pid, output captured, exit code reported |
| `aos run` on `/bin/sh` | refused, and the allowlist is named in the message |
| audit log after both runs | two lines, one allowed and one refused with a reason |
| kill switch | three sleeping agents, all three stopped, none left running |
| shell metacharacters in an argument | stayed literal, the injected `touch` never ran |

The refusal line in the audit log matters more than the success line. A log that only records
what worked hides exactly the calls worth reviewing.

## mistakes and their permanent guards

Full detail in [bug-list.md](bug-list.md).

| mistake | permanent guard |
|---|---|
| agent output piped with no reader, so output was lost and a chatty agent would deadlock | `a_chatty_agent_finishes_and_its_output_is_kept`, which pushes 1.3 MB through, plus `agent_output_lands_in_its_log` |

One bug so far, and it was found by running the binary rather than by a test. That is the
honest lesson of the session. The test suite was green while the thing was broken, because
every test used a program that printed almost nothing. Tests written from the same assumption
as the code do not catch the assumption being wrong.

The guard was checked by reverting the fix and watching the test fail, which is now the rule
for every entry in the bug list.

## deadlines

Nothing is late, because nothing has a hard date yet. Phase 1 is the next chunk and is
estimated at a week of part time work.

## what was learned

Rust's type system did real work here. `AgentId` cannot exist without passing its check, so
path traversal is refused once at construction rather than at every place an id becomes a
filename. That is a guard the compiler enforces rather than a rule someone has to remember.

Linux gives away for free what Windows charged a kernel driver for. Process state is readable
under `/proc`, and resource limits are cgroups rather than a signed minifilter. Phase 4 is a
week here and was phase 7 and optional on Windows.

Running the thing is not optional. The pipe bug was invisible to a green test suite.
