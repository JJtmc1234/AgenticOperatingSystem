# Agentic Operating System (AOS)

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

## policy

Copy [examples/policy.toml](examples/policy.toml) into your run directory to change what is
allowed. With no file at all, read runs freely and everything else needs a commit.

Above tier read, `aos start` tells you what would happen and stops:

```
$ aos start examples/risky.json
tier destructive needs a commit, so nothing has run.

  risky would run /usr/bin/sleep ["30"] at tier destructive

To go ahead:
  aos start examples/risky.json --commit 8a59a17dfd7d8e64f166eaf98c69b3cd
```

`examples/risky.json` only sleeps. It is declared destructive so the handshake can be tried
without anything at stake.

The plan records the exact request it was made for, so planning one thing and committing
another is refused. It is single use, it expires after two minutes, and it does not survive a
daemon restart, because an offer the current daemon never made is not one it should honour.

## the event log

`run/events.jsonl` is the only durable state. Everything the runtime believes is a fold over
it, so `aos status` can answer what is running after a crash, and can do it by reading a text
file rather than trusting memory that no longer exists.

Each start record carries a `start_token` next to the pid, which is the process start time
from `/proc`. Linux reuses pids, so a pid alone cannot tell your agent apart from whatever
took its number afterwards. `aos status` reports a pid it cannot confirm as lost and leaves
it alone rather than risk signalling a stranger.

## what works today

Phases 0, 1 and 2. Contracts, the supervisor, the event log with replay, adoption of agents
that outlived their supervisor, `aosd` with `list` and `stop` working from any terminal, and
policy with the plan then commit handshake.

Phase 3a as well: `aos-files`, the file capability server, spoken over MCP. Every call is
gated by scope, policy and the same plan then commit handshake, and every gated call is
recorded including the refusals. The scope is two directories rather than one: everything an
agent may read, and the narrower place its changes may land.

3b, the command runner, is not started. It is the harder half, because an allowlist bounds
which binary starts and not what that binary then does. Never a raw shell.

## the file server

MCP is the protocol Claude Code already speaks, so a capability server is the way an agent
gets to touch files without ever being handed a shell.

```sh
mkdir -p run/work/task
cp examples/policy.toml run/policy.toml

./target/debug/aos-files \
  --root "$PWD/run/work" \
  --write-root "$PWD/run/work/task" \
  --policy "$PWD/run/policy.toml" \
  --log "$PWD/run/events.jsonl"
```

`--root` is everything it may read and has to exist already. `--write-root` is the one
directory inside it where changes may land, and it has to exist too. Leaving it out means
nothing may be changed at all, which is the safe default rather than an oversight: defaulting
to the whole read root would make forgetting the flag look exactly like deciding to allow it.

Reading and changing are separate grants because they are separate sizes. A worker is given a
project to read, since work that cannot see the code around it is work done blind, and one task
workspace to change, since a worker that can write anywhere it can read is a worker whose
mistake is unbounded.

`--policy` decides what is allowed. `--log` is optional, and leaving it out means nothing is
recorded, which is almost never what you want. `--agent` defaults to `claude-code` and is the
name the policy singles out in its `[agents]` table.

Some names are refused wherever they sit, whatever the roots say. A private key inside a project
directory is inside the root by every check the path resolver makes, and handing it over would
still be the worst thing this server could do. `.ssh`, `.aws`, `.gnupg`, `.env`, `.netrc`,
`.npmrc`, `.git-credentials`, `credentials`, `secrets`, `.carl`, `.claude`, anything starting
`id_rsa` or `id_ed25519`, and anything ending `.pem`, `.key`, `.p12`, `.pfx` or `.keystore`. The
check is on the name asked for and on the name it resolves to, so a link called `notes.txt`
landing on `id_rsa` is caught as well.

It speaks MCP on stdio, so running it by hand gets you a server waiting for JSON on its input
rather than anything to look at. To give it to Claude Code, add it as an MCP server:

```sh
claude mcp add aos-files -- \
  /absolute/path/to/target/debug/aos-files \
  --root /absolute/path/to/run/work \
  --write-root /absolute/path/to/run/work/task \
  --policy /absolute/path/to/run/policy.toml \
  --log /absolute/path/to/run/events.jsonl
```

Absolute paths throughout, because the server is started by Claude Code from a working
directory you did not choose.

| capability | tier | needs a commit under the default policy |
|---|---|---|
| `list_dir` | read | no |
| `read_file` | read | no |
| `find` | read | no |
| `write_file` | write | yes |
| `make_dir` | write | yes |
| `move_file` | write | yes |
| `delete_file` | destructive | yes |

The read three run freely because they change nothing. Everything else returns a plan the
first time and acts only when a second call quotes that plan's id, which is the same handshake
`aos start` uses and for the same reason.

## before you edit

Read [infrastructure.md](infrastructure.md) first, and the section on why an allowlist is not
enough on its own. Never add an interpreter to `allowed-programs.json`. `python3 -c` and
`node -e` take code on their own argument vector, so allowing one grants everything the rest
of the gates protect.

Runtime state lives in `run/` and is never committed.
