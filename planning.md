# planning

The idea broken into chunks. Effort assumes part time solo work. Dates are targets, not
commitments.

## principles that order the phases

Build the part with the highest cost of being wrong first. That is the supervisor, because it
owns process lifetimes and a mistake there leaves things running on the machine.

The model never calls a tool directly. Every call goes through a gate that validates, checks
policy, executes and reports. This is why the contracts crate exists before any capability.

A bug is finished when a permanent guard exists that would have caught it. Every entry in
[bug-list.md](bug-list.md) names the test that fails against the old code.

Nothing is called done on a compile. Done means `cargo fmt`, `cargo clippy --all-targets
-- -D warnings` and `cargo test` all run, plus the thing exercised for real.

## phases

| phase | chunk | effort | status |
|---|---|---|---|
| 0 | contracts crate, supervisor, cli, event log | 1 session | done |
| 0r | event log as source of truth, replay, pid identity | 1 session | done |
| 1a | adoption, so a restart re-takes its surviving agents | 1 session | done |
| 1b | the daemon and its socket | 1 week | not started |
| 2 | policy engine and the plan then commit handshake | 1 week | not started |
| 3 | capability servers over MCP, files and shell first | 1 to 2 weeks | not started |
| 4 | resource limits through cgroups | 1 week | not started |
| 5 | routines and a scheduler, starting with a daily brief | 1 to 2 weeks | not started |
| 6 | sensors, so agents react to the machine rather than to prompts | 2 weeks | not started |

## phase 0, foundation

`aos-core` holds the safety contracts and touches no processes. `aos-supervisor` starts,
watches and stops agents as child processes. `aos-cli` runs one in the foreground and writes
the event log.

Passes when `cargo test` runs green against real child processes, and `aos run` starts a real
agent, keeps its output, and refuses a program that is not on the allowlist. Both verified.

## phase 0r, the log becomes the source of truth

Added after reading Hunter's `agentic_os`. His kernel keeps its state as a fold over an
append only log and replays it on boot. The log here was write only, which meant phase 1 had
no honest answer for what a daemon should do after a crash.

Now every belief is a fold over `run/events.jsonl`, and `aos status` reconciles that fold
against `/proc`. Because Linux reuses pids, each start record carries the process start time
alongside the pid, and an agent is adopted only when both match.

Passes when a supervisor is killed mid run, the agent survives as an orphan, `aos status`
reports it alive, and the same command reports it lost once the agent is gone. Both verified,
plus a live process with a tampered token which recovery correctly refuses to claim.

## phase 1a, adoption

A supervisor that restarts has to take back the agents that outlived it. They are no longer
its children, so it cannot wait on them, and signalling them by number races against pid
reuse.

`Supervisor::adopt` pins each survivor with a pidfd, which cannot come to mean a different
process. `adopt_from` drives that off a replayed log and reports what was lost.

Passes when a genuinely orphaned process, reparented to init, is adopted by a fresh
supervisor, reported running, and stopped through its descriptor, and when a handle with a
stale token is refused while the real process is left untouched. Both verified.

## phase 1b, the daemon

Phase 0 supervises in the foreground only, so `list` and `stop` cannot see an agent another
invocation started. A long lived `aosd` holds the supervisor and the cli talks to it over a
unix socket.

Phases 0r and 1a did the hard half. The daemon boots by replaying the log rather than starting
blind, and adopts only the agents it can prove are still its own.

The socket is the new attack surface, so it gets file permissions restricted to the owner and
a message schema that is checked before anything is acted on.

Passes when an agent started in one terminal is visible and stoppable from another, when
restarting the daemon re-adopts a surviving agent rather than losing or duplicating it, and
when killing the daemon does not orphan its agents silently.

## phase 2, policy

Tiers exist as types already. This phase gives them a policy file and a verdict of allow,
prompt or deny, plus the plan then commit handshake so nothing mutating happens in one step.

Done when a `Destructive` action refuses to run without an explicit commit, and the refusal
is in the audit log with a reason.

## phase 3, capability servers

Rebuild the useful part of the Windows AOS in Rust over MCP, which is the protocol Claude
Code already speaks. Files first, then a policy gated command runner. Never a raw shell.

Done when a request like "find every PDF I touched this week and file them by project" works
from Claude Code, with every gated call in the audit log.

## phase 4, resource limits

cgroups v2 gives memory and cpu ceilings per agent. This is the thing Windows made hard and
Linux makes routine, so it lands early rather than late.

Done when an agent that tries to allocate past its ceiling is stopped by the kernel and the
supervisor reports why.

## phase 5, routines

Declarative routines on a schedule. The daily brief comes first, because on Windows it was the
first thing that paid for itself without being asked.

Done when the brief runs unattended and gets read.

## phase 6, sensors

inotify for filesystem activity, `/proc` for process activity, journald for system events.
Agents react to what happened rather than to being asked.

Done when a routine fires from an event and the result is useful rather than noisy.

## known risks

| risk | response |
|---|---|
| Rust slows early progress | phase 0 was deliberately small, and it is done |
| the daemon orphans agents on crash | phase 1 acceptance test covers exactly this |
| scope grows toward a custom distro | brainstorm records that as rejected for now, revisit only after phase 5 |
