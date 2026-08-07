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

cat run/logs/hello.log     # what the agent said
cat run/audit.jsonl        # what the runtime allowed or refused
```

`examples/blocked.json` asks for `/bin/sh`, which is not on the allowlist. It is refused and
the refusal is written to the audit log.

## what works today

Phase 0. Contracts, the supervisor, the `aos` binary, the audit log. Supervision is foreground
only, so `list` and `stop` across separate invocations wait for the phase 1 daemon.

## before you edit

Read [infrastructure.md](infrastructure.md) first, and the section on why an allowlist is not
enough on its own. Never add an interpreter to `allowed-programs.json`. `python3 -c` and
`node -e` take code on their own argument vector, so allowing one grants everything the rest
of the gates protect.

Runtime state lives in `run/` and is never committed.
