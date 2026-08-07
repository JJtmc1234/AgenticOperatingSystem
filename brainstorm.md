# brainstorm

How this project idea was reached.

## the want

Agents should be part of the operating system, not an app you alt tab into. They should be
started, watched, limited and stopped the way the system already does that for processes.
Goal is JJ's own productivity, not a product for other people.

## why this is the second attempt

The first AOS targeted Windows. It reached a working capability broker, three MCP servers
and a daily brief, then stopped early. Two things changed.

A new computer arrived, running Ubuntu 26.04. AOS follows the machine it is meant to
improve, so a Windows layer on a Linux desktop is a layer over nothing.

JJ's CS teacher calls Linux the hacker OS, and for this project that is literally true.
Everything an agent supervisor wants to inspect is already readable on Linux. Processes are
files under `/proc`. Limits are cgroups. Isolation is namespaces. Service supervision is a
solved and documented pattern in systemd. On Windows the same information sat behind ETW,
WMI and eventually a signed kernel driver.

## four readings of "agentic operating system"

| reading | verdict |
|---|---|
| ordinary desktop app | rejected, no system integration, and this was rejected the first time too |
| agent runtime layered on Linux | chosen |
| replace the shell or desktop environment | rejected for now, high risk and it rebuilds work that is not the point |
| custom kernel or distro | rejected for now, the slowest possible iteration loop |

Chosen: an agent runtime on top of an ordinary Ubuntu install. A custom image stays possible
later, and Linux makes it far cheaper than a Windows WIM ever was.

## what carries over and what does not

The safety design carries over whole, because it was the good part. Risk tiers, plan then
commit, an allowlist that refuses interpreters, an append only audit log, a kill switch, and
post condition checks after a change lands.

The platform work does not carry over. UIAutomation, the WIM build, the minifilter and the
signing problem are all gone. That is most of why the rewrite is cheap.

The old tree is kept as `old-windows-code.zip` at the repo root, taken with `git archive` so
it holds what was committed rather than whatever happened to be on disk. That distinction was
the lesson of JJtorio issue 16.

## the language choice

Rust. The supervisor is the part that can leave stray processes on the machine, so the
component with the highest cost of being wrong is the one being written first. Rust also
removes an entire class of mistake from the part that handles other people's process
lifetimes, and it compiles to a single binary with no runtime to install.

Cost is honest: Rust is harder than Python, and progress will be slower at the start.

## what has to be true first

The supervisor has to be boring and correct before anything clever sits on it. An agent
runtime that occasionally leaks a process is not a foundation. Phase 0 therefore builds the
contracts and the supervisor and nothing else.

See [planning.md](planning.md) for the phases.
