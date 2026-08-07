# infrastructure

Holistic view of the system. Components, how they talk, and what moves between them.

## framing: this is a harness

AOS is an agent harness. The model reasons and the harness acts. The harness is what decides
whether the thing is reliable.

The central rule is that the model never calls a tool directly. It emits a structured request.
The harness validates the shape, checks policy, executes, and hands the result back. Anything
that reaches the machine without passing that gate is a bug.

## component map

```
  cli, aos                        phase 0, done
    validate, run
        |
        |  unix socket, JSON      phase 1
        v
  daemon, aosd                    phase 1
    holds the supervisor, one owner of every child
        |
        v
  policy                          phase 2
    tier to verdict, plan then commit handshake
        |
        v
  supervisor                      phase 0, done
    start, state, list, stop, stop_all
        |
        |  fork and execve, no shell
        v
  agent processes
    stdout and stderr to run/logs/<id>.log
    cgroup per agent                phase 4

  capability servers              phase 3
    MCP over stdio, files and a gated command runner

  sensors                         phase 6
    inotify, /proc, journald  ->  event bus  ->  routines
```

## crates

| crate | job | depends on |
|---|---|---|
| `aos-core` | contracts only. Tiers, agent ids, specs, audit entries. No side effects. | nothing |
| `aos-supervisor` | child process lifetimes | `aos-core`, libc |
| `aos-cli` | the `aos` binary | both |

`aos-core` deliberately touches nothing, so the rules can be tested without spawning
anything. That is why nine of its tests run in under a millisecond.

## data that moves between components

| from | to | transport | payload |
|---|---|---|---|
| user | cli | argv | subcommand and a path to a spec |
| cli | supervisor | in process | `AgentSpec` |
| supervisor | kernel | execve | program path and an argument list, never a command string |
| agent | disk | file descriptor | stdout and stderr, combined, appended |
| cli | disk | file | one `AuditEntry` per attempt, JSON per line |
| cli | daemon | unix socket, JSON | phase 1 |

## the safety gates that exist today

| gate | where | what it refuses |
|---|---|---|
| agent id validation | `aos_core::AgentId::new` | anything with a separator, a dot segment, an uppercase letter or a null byte |
| program allowlist | `Supervisor::start` | any program not named exactly |
| no shell | `Command::args` | metacharacters stay literal, because there is no shell to interpret them |
| stdin closed | `Stdio::null` | a child eating the parent's input |
| bounded wait | `signal::wait_bounded` | an unbounded hang on a child that ignores SIGTERM |
| audit on refusal | `runtime::run` | a log that only records successes |

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
| observability and tracing | done. Append only JSONL, one entry per attempt including refusals. |
| human in the loop | not started, phase 2 |
| verification | partial. Every bug has a regression test. Post conditions are not built. |
| agent loop, planning, memory, orchestration | not started, phase 5 and beyond |

Five of these were done on the Windows AOS and are being rebuilt rather than invented. The
design is known, so the risk here is effort and not uncertainty.
