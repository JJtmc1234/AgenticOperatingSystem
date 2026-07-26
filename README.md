# AgenticOS

An agent-native layer over Windows, built to be baked into a custom Windows image.

The guiding decision: **the custom image is a packaging target, not a development
environment.** Everything is built as idempotent provisioning modules that install onto a
live machine and get dogfooded daily. The image build applies those same tested modules
offline to a mounted WIM, which keeps it cheap. See the plan for full phasing.

## Layout

| Path | What it is |
|---|---|
| `src/Aos.Core/` | Safety contracts: risk tiers, capability descriptors, audit entries, arg coercion |
| `src/Aos.Broker/` | The gate every capability call passes through: policy, audit, dry-run, snapshots |
| `src/Aos.Mcp.Windows/` | MCP server: windows, processes, UIAutomation, screen capture |
| `provisioning/` | **The definition of the OS.** Idempotent Test/Set modules |
| `policy/default.yaml` | Capability policy the broker enforces |
| `tests/` | Broker, policy, path guard, redaction, arg coercion |

## Getting started

```powershell
dotnet build aos.sln
dotnet test aos.sln
.\provisioning\Install-Aos.ps1 -WhatIf   # report what would change
.\provisioning\Install-Aos.ps1           # publish servers, install policy, register MCP
```

Runtime state lives in `%LOCALAPPDATA%\AgenticOS` (`audit/`, `trash/`, `data/`, `bin/`,
`policy.yaml`) and is never committed.

Claude Code reads the servers from `.mcp.json` in this repo. Claude Desktop is registered
by `provisioning/modules/30-mcp.ps1`, which merges into its config and backs it up first.

## The safety model

Every capability declares a `RiskTier`, and the broker derives its guarantees from that
tier rather than trusting each capability to opt in:

| Tier | Meaning | Default policy |
|---|---|---|
| `Read` | Observes only | auto-allow |
| `Write` | Recoverable user-data change | plan, then commit |
| `System` | Machine or OS state | plan, then commit, restore point first |
| `Destructive` | Data loss or hard to reverse | plan, then commit, restore point first |

Properties enforced structurally, not by convention:

- **Plan-then-commit.** Anything not plainly allowed returns a plan and changes nothing.
  Applying requires an explicit second call with `commit: true`.
- **Fails closed.** A missing tier rule, malformed verdict, or unknown capability id denies.
  An unknown *tier name* in policy throws at load, so a typo cannot leave a tier unpoliced.
- **Policy cannot weaken the handshake.** Omitting `dryRunOnly` for `System` does not
  disable it; `RequiresCommit` is derived from the tier.
- **No restore point, no commit.** `System`+ commits are refused when snapshots are
  unavailable, which is the default unelevated — VSS needs admin. Capabilities a shadow copy
  cannot protect (killing a process) opt out explicitly with a stated reason.
- **Always audited.** One append-only JSONL entry per call including denials, unknown ids,
  and exceptions. A failed audit write fails the call.
- **Secrets never reach the log.** Credential-shaped keys are redacted, including whole
  subtrees.

## Gotchas worth knowing before you edit

**Provisioning module scoping.** Steps are collected from a module, then invoked after that
module's scope is gone. So:

- Module-local *variables* referenced by a step must be captured with `.GetNewClosure()`.
- Because `GetNewClosure` snapshots the module scope, runner-scope *functions* would not
  resolve from inside a closure either. Shared helpers are therefore defined `global:` and
  removed on exit.

Both rules are documented at the top of `provisioning/Install-Aos.ps1`. Getting either
wrong fails at step-execution time, not at parse time.

**`System.IO` is not in the implicit using set** for WindowsDesktop SDK projects
(`UseWPF`/`UseWindowsForms`), though it is for plain `net9.0`. Import it explicitly.

**`JsonValue.GetValue<T>()` demands an exact type match** for nodes built in-process, so a
value constructed from `int` throws when read as `long` — while the same argument arriving
over the wire is JsonElement-backed and converts freely. Always go through
`Aos.Core.JsonArgs`, never `GetValue<T>()` directly.

**UIAutomation refs.** `ui.tree` addresses elements by a path of child indices (`"0.3.1"`)
rather than display name, because names repeat, localize, and contain whitespace. Refs go
stale when the UI changes; the error says so and tells you to re-read the tree.
