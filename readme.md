# aos

Agent native layer over Linux. Rust, developed on Ubuntu 26.04.

Agents are supervised like processes. The runtime starts them, bounds what they may run,
records every attempt including refusals, and can stop all of them at once.

This is the second AOS. The first targeted Windows in C sharp and is archived here as
`old-windows-code.zip`. Its safety design carried over. Its platform did not.

## docs

| file | contents |
|---|---|
| [brainstorm.md](brainstorm.md) | how the idea was reached and why Windows was left behind |
| [planning.md](planning.md) | the phases, effort, and acceptance criteria |
| [infrastructure.md](infrastructure.md) | components, how they talk, the safety gates |
| [progress-report.md](progress-report.md) | current status and what was learned |
| [bug-list.md](bug-list.md) | every bug and the test that stops it coming back |

## getting started

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
```

Then run a real agent. The run directory holds the allowlist, the audit log and agent output.

```sh
mkdir -p run
echo '["/usr/bin/echo"]' > run/allowed-programs.json

./target/debug/aos validate examples/hello.json
./target/debug/aos run examples/hello.json
./target/debug/aos status

cat run/logs/hello.log     # what the agent said
cat run/events.jsonl       # what the runtime did, including what it refused
```

`examples/blocked.json` asks for `/bin/sh`, which is not on the allowlist. It is refused and
the refusal is written to the event log with its reason.

## the daemon

`aos run` supervises in the foreground, so it only lasts as long as your terminal. `aosd` is
the long lived version, and it is the only thing that owns an agent.

```sh
./target/debug/aosd &          # one per run directory

./target/debug/aos ping
./target/debug/aos start examples/sleeper.json
./target/debug/aos list        # works from any terminal
./target/debug/aos stop sleeper
./target/debug/aos stop-all    # the kill switch
```

Stopping the daemon does not stop its agents. They keep running, and the next `aosd` replays
the log and adopts them, which `aos list` marks as `(adopted)`. An adopted agent has no exit
code to report, because init reaped it.

The socket is `run/aosd.sock`, mode 0600, and it only answers connections from your own user.

## the event log

`run/events.jsonl` is the only durable state. Everything the runtime believes is a fold over
it, so `aos status` can answer what is running after a crash, and can do it by reading a text
file rather than trusting memory that no longer exists.

Each start record carries a `start_token` next to the pid, which is the process start time
from `/proc`. Linux reuses pids, so a pid alone cannot tell your agent apart from whatever
took its number afterwards. `aos status` reports a pid it cannot confirm as lost and leaves
it alone rather than risk signalling a stranger.

## what works today

Phases 0 and 1. Contracts, the supervisor, the event log with replay, adoption of agents that
outlived their supervisor, and `aosd` with `list` and `stop` working from any terminal.

Phase 2 is next: a policy file, verdicts of allow, prompt or deny, and the plan then commit
handshake so nothing mutating happens in one step.

## before you edit

Read [infrastructure.md](infrastructure.md) first, and the section on why an allowlist is not
enough on its own. Never add an interpreter to `allowed-programs.json`. `python3 -c` and
`node -e` take code on their own argument vector, so allowing one grants everything the rest
of the gates protect.

Runtime state lives in `run/` and is never committed.
