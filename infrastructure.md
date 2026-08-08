# infrastructure

Holistic view of the system. Components, how they talk, and what moves between them.

## framing: this is a harness

AOS is an agent harness. The model reasons and the harness acts. The harness is what decides
whether the thing is reliable.

The central rule is that the model never calls a tool directly. It emits a structured request.
The harness validates the shape, checks policy, executes, and hands the result back. Anything
that reaches the machine without passing that gate is a bug.

## the event log is the only durable state

Everything the supervisor believes is a fold over `run/events.jsonl`. Append first, then
change the belief, never the other way round. Nothing else remembers what was running, so a
supervisor starting up asks the log instead of guessing.

This is taken from Hunter's `agentic_os` kernel, which states the rule as append, fold, then
broadcast, in that order, always. Before reading it, the log here was write only, and the
plan had no honest answer for what a daemon should do after a crash. Now it does.

One difference. His kernel uses SQLite. This uses one JSON object per line, because the log
has to be readable with `cat` at the moment the thing that writes it is the thing that is
broken.

| event | meaning |
|---|---|
| `started` | launched, and believed running from here on |
| `exited` | ended on its own |
| `stopped` | the supervisor ended it |
| `refused` | never ran, and why |
| `lost_while_unsupervised` | replay found a start with no ending and the process is gone |

`believed_running` folds those into the set of agents with a `started` and no ending. That
fold is pure and lives in `aos-core`, so it is tested without spawning anything.

## a pid is not an identity

Linux reuses pids. After a crash, the pid the log remembers may belong to a completely
unrelated process, so acting on it would mean signalling a stranger.

Every `started` record therefore carries a `start_token` next to the pid: the process start
time in clock ticks since boot, field 22 of `/proc/<pid>/stat`. The kernel sets it once and
never changes it, so the pair identifies a process for as long as the machine is up.

Replay adopts an agent only when both match. A pid that is alive under a different token is
reported as lost and never touched. Verified against a real live process by tampering with
the token in the log and watching recovery refuse to claim it.

Hunter's kernel never meets this problem, because his agents are rows in a database rather
than processes on a machine. It is the cost of AOS supervising the real thing.

## the token is enough to decide, not enough to act

Checking the token tells you a pid is still yours. It does not keep it yours. Between the
check passing and a signal being sent, the process can exit and its number can be handed to
something else. The window is tiny, the consequence is killing the wrong program, and the bug
would never reproduce.

So AOS never signals an inherited agent by number. `PidFd::open` pins the process behind a
file descriptor, and a pidfd never comes to mean a different process. A signal sent through
it either reaches the process we opened or fails because that process is gone.

The open itself is check, open, check again. If the token still matches while the descriptor
is already held, the descriptor belongs to the process we meant, because nothing can swap it
afterwards. Without the second check the swap could have happened during the open.

| situation | what identifies the process |
|---|---|
| deciding, during replay | pid plus start token from `/proc` |
| acting, on an inherited agent | a pidfd, opened under the check above |
| acting, on an agent we spawned | the `Child` we already hold |

## the daemon and its socket

`aosd` is the only thing that owns an agent. Two owners would mean two answers to what is
running, and the whole point of the log is that there is one.

Shutting the daemon down does not stop its agents. They keep running and the next boot adopts
them. A supervisor that killed everything whenever it restarted would make restarts
frightening, and a frightening restart is one nobody performs.

The socket is the entire attack surface. Anyone who can reach it can start and stop processes
as this user, so it gets four separate guards.

| guard | stops |
|---|---|
| the run directory is 0700 | anyone else reaching any name inside it at all |
| bound under a staging name, chmodded, then renamed into place | a window where the real socket path exists with the wrong mode |
| mode asserted as 0600 afterwards, else refuse to serve | a filesystem that quietly ignored the chmod |
| `SO_PEERCRED` checked per connection | a caller who is not this user, even if the mode were loosened by mistake |
| the request schema parsed before anything acts | a path shaped agent id, or a verb that does not exist |

Creating the socket and then fixing its permissions would leave a gap, and a gap is all an
attack needs. The obvious fix is to narrow the umask across the bind, and it is wrong: `umask`
is per process, so it changes what every other thread creates at the same moment. See bug 4.
Rename is atomic and affects nothing outside this call, so that is what is used instead.

A socket file that already exists is either a live daemon or debris from a crashed one.
Guessing either way is wrong. Deleting a live daemon's socket strands it, and refusing to
start over a dead one's leftovers demands a manual cleanup after every crash. So `aosd` asks:
if something answers, it refuses to start. If nothing answers, the file is debris and is
cleared.

Requests are served one at a time. The daemon owns process lifetimes, so two requests racing
to start the same agent is a bug with a stray process at the end of it. Serialising makes it
impossible, and nothing here is slow enough to mind.

A malformed request is answered with an error, never dropped. A caller that gets silence
cannot tell a rejection from a crash.

### the one ordering that matters

Append to the log, then tell the supervisor. Never the other way round.

If a start is recorded and the process then fails, the log claims one thing too many, and
replay checks against `/proc` and corrects it. If a process starts and the record fails, the
log claims one thing too few, and nothing will ever know that process exists. So a start whose
record cannot be written is stopped again immediately rather than left unaccounted for.

## the two kinds of agent

| kind | we can | we cannot |
|---|---|---|
| spawned | wait on it, read its exit code, signal it | |
| adopted | see whether it lives, signal it | learn how it ended |

An adopted agent outlived a previous supervisor, so the kernel reparented it to init, and
init reaped it. Its exit code went there. `AgentState::Stopped { code: None }` says so
honestly rather than inventing a zero that would read as success.

Parsing field 22 has its own trap. Field 2 is the executable name in parentheses and may
itself contain spaces and parentheses, so splitting the line on whitespace is wrong. The
parser reads everything after the last `)`, and there is a test with a process named
`we ) love ) parens` to keep it that way.

## component map

```
  cli, aos                        phase 0, done
    validate, run, status         read files, need no daemon
    ping, list, start, stop, stop-all
        |
        |  unix socket, one JSON object per line, owner only
        v
  daemon, aosd                    phase 1b, done
    the only owner of an agent. Boots by replaying the log.
        |
        v
  policy                          phase 2
    tier to verdict, plan then commit handshake
        |
        v
  supervisor                      phase 0, done
    start, state, list, stop, stop_all
        |                     \
        |                      \  replay: log folded, then checked against /proc
        |  fork and execve      \
        |  no shell              -> recover -> alive or lost
        v
  agent processes
    stdout and stderr to run/logs/<id>.log
    cgroup per agent                phase 4

  event log, run/events.jsonl     phase 0, done
    the only durable state. Every belief is a fold over this file.

  capability servers              phase 3
    MCP over stdio, files and a gated command runner

  sensors                         phase 6
    inotify, /proc, journald  ->  event bus  ->  routines
```

## crates

| crate | job | depends on |
|---|---|---|
| `aos-core` | contracts, the event log, the protocol. No side effects on processes. | nothing |
| `aos-supervisor` | process lifetimes, `/proc` identity, pidfds, replay | `aos-core`, libc |
| `aosd` | the daemon. Owns the supervisor and the socket. | both, libc |
| `aos-cli` | the `aos` binary. Talks to the daemon. | both |

The protocol lives in `aos-core` so both sides compile against one definition. A protocol
defined twice is a protocol that drifts.

`aos-core` deliberately touches no processes, so the rules can be tested without spawning
anything. That is why fifteen of its tests run in under a millisecond.

## data that moves between components

| from | to | transport | payload |
|---|---|---|---|
| user | cli | argv | subcommand and a path to a spec |
| cli | supervisor | in process | `AgentSpec` |
| supervisor | kernel | execve | program path and an argument list, never a command string |
| supervisor | kernel | `/proc/<pid>/stat` | start time, to confirm a pid is still the same process |
| agent | disk | file descriptor | stdout and stderr, combined, appended |
| cli | disk | `run/events.jsonl` | one `Record` per attempt, JSON per line |
| cli | daemon | unix socket, JSON | phase 1 |

## the safety gates that exist today

| gate | where | what it refuses |
|---|---|---|
| agent id validation | `aos_core::AgentId::new` | anything with a separator, a dot segment, an uppercase letter or a null byte |
| program allowlist | `Supervisor::start` | any program not named exactly |
| no shell | `Command::args` | metacharacters stay literal, because there is no shell to interpret them |
| stdin closed | `Stdio::null` | a child eating the parent's input |
| bounded wait | `signal::wait_bounded` | an unbounded hang on a child that ignores SIGTERM |
| record on refusal | `runtime::run` | a log that only records successes |
| pid identity | `proc::is_still` | adopting a recycled pid that now belongs to a stranger |
| pinned signalling | `PidFd::open` | a pid recycled between the check and the signal |

The agent id check is worth calling out. Ids become filenames under the run directory, so
they are validated once at construction rather than guarded at every use site. `AgentId` can
only be built through the checked constructor, and `serde` is wired to the same constructor
so a hand edited spec file cannot get around it.

## why an allowlist is not enough on its own

An allowlist bounds which binary starts, not what that binary then does. Any interpreter
accepts code on its own argument vector with no shell involved.

```
python3 -c "..."     node -e "..."     git -c alias.x='!...' x
```

So listing an interpreter grants arbitrary code execution as the user, which silently confers
every capability the other gates protect. On the Windows AOS node, python, npm and dotnet were
on the list and were removed for exactly this reason. Do not add them back.

## the harness primitives, honestly

| primitive | state |
|---|---|
| tool design | not started, phase 3 |
| permissions and authorization | partial. Allowlist and tiers exist, policy verdicts do not. |
| observability and tracing | done. Append only JSONL, one record per attempt including refusals. |
| memory and state | partial. The log replays, so state survives a crash. Agent memory is phase 5. |
| human in the loop | not started, phase 2 |
| verification | partial. Every bug has a regression test. Post conditions are not built. |
| agent loop, planning, orchestration | not started, phase 5 and beyond |

Five of these were done on the Windows AOS and are being rebuilt rather than invented. The
design is known, so the risk here is effort and not uncertainty.

## what else is worth taking from agentic_os

Hunter's repo solves a different problem. His kernel coordinates a team of Claude Code
instances arranged as an org chart. AOS supervises processes and gates what they may do. The
overlap is smaller than the two names suggest, so most of this is borrowed as a principle
rather than as code.

| his idea | verdict here |
|---|---|
| state is a fold over an append only log | taken, and it reshaped phase 1 |
| structure enforced in code, not in prompts | already agreed. His kernel rejects sibling messages, `AgentId` refuses path shaped names. Worth stating out loud as the shared rule. |
| rejections are logged, never silently dropped | already agreed. His `message_rejected` is our `refused`. |
| every phase states its own passes when | taken. The acceptance criteria in planning.md now read that way. |
| approvals routed to one place holding a policy file | fits phase 2. The tier decides the verdict, one place resolves the prompt. |
| one git worktree per worker so parallel work cannot collide | park until phase 5. Nothing here spawns concurrent writers yet. |
| a spatial canvas over a WebSocket | not now. The log is the interface, and a picture of one agent is not worth a UI. |
| memory owned by a manager, proposed by workers | park until phase 5. It presumes an org, and AOS has no org. |

The most useful disagreement is the kill switch. He defers it and token budgets past v1. AOS
has a kill switch in phase 0, because a runtime that starts real processes and cannot stop
all of them is not something to leave running on your own machine. That difference falls
straight out of the two projects supervising different things.
