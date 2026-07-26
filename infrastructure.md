# infrastructure

Holistic view of the system. Components, how they talk, and what moves between them.

## component map

```
custom Windows image (phase 6)
  provisioning modules applied offline to a mounted WIM

  heads up display (phase 3)
    C sharp WebView2 host: overlay, tray, global hotkeys
        |
        |  local WebSocket, JSON
        v
  orchestrator (phase 2)
    TypeScript, Claude Agent SDK, routines, memory, scheduler
        |
        |  MCP over stdio, JSON RPC
        v
  capability servers (phase 1)
    aos-windows   aos-files   aos-shell   aos-apps
        |
        |  in process call
        v
      broker
        policy, audit, dry run, snapshots, kill switch
        |
        v
      Windows APIs: UIAutomation, Win32, filesystem, registry, Google APIs, CDP

  sensors service (phase 4)
    ETW, USN journal, WMI, window focus  ->  event bus  ->  orchestrator

  vision fallback (phase 5)      kernel minifilter (phase 7, optional, VM only)
    screenshot -> SendInput        filesystem events -> sensors service
```

## why MCP is the boundary

One protocol serves three consumers: Claude Code, Claude Desktop, and the phase 2
orchestrator. A capability written once is immediately usable in tools already installed,
so phase 1 pays for itself before any user interface exists. It also gives a process
boundary per server for sandboxing and per server policy.

## data that moves between components

| from | to | transport | payload |
|---|---|---|---|
| Claude client or orchestrator | capability server | MCP stdio, JSON RPC | tool name, arguments, commit flag, reason |
| capability server | broker | in process | `CapabilityRequest` |
| broker | capability | in process | arguments plus a dry run flag |
| broker | audit log | append only file | one JSONL `AuditEntry` per call |
| sensors service | orchestrator | event bus | filesystem, process, and focus events |
| display | orchestrator | local WebSocket | intent text, approval verdicts, halt signal |

## outcome envelope

Every tool returns the same shape, so denials and plans are data the model can read rather
than errors it has to guess at.

```json
{ "status": "Succeeded | DryRun | Denied | Failed", "message": "...", "result": { } }
```

## repository layout

| path | contents |
|---|---|
| `src/Aos.Core/` | risk tiers, capability contracts, audit types, argument coercion |
| `src/Aos.Broker/` | policy evaluation, audit sink, path guard, redaction, snapshots |
| `src/Aos.Mcp.Windows/` | MCP server for windows, processes, UIAutomation, capture |
| `provisioning/` | the definition of the OS, idempotent Test and Set modules |
| `policy/default.yaml` | capability policy the broker enforces |
| `tests/` | broker, policy, path guard, redaction, argument coercion |

`provisioning/` is load bearing. It is the actual definition of the operating system, and
`image/` is only a consumer of it.

## runtime state

Lives in `%LOCALAPPDATA%\AgenticOS` and is never committed.

| directory | contents |
|---|---|
| `audit/` | append only JSONL, one file per UTC day |
| `trash/` | staged deletes with a manifest, so nothing is really deleted |
| `data/` | agent memory in SQLite, screenshots |
| `bin/` | published capability servers |
| `policy.yaml` | the installed copy the broker reads at startup |

The installed policy is deliberately a copy, not a link to the repo. Local tightening
should not show up as a dirty working tree, and an image build must bake a fixed policy.

## the safety model

Every capability declares a risk tier. The broker derives its guarantees from the tier
rather than trusting each capability to opt in.

| tier | meaning | default policy |
|---|---|---|
| `Read` | observes only | auto allow |
| `Write` | recoverable change to user data | plan, then commit |
| `System` | machine or OS state | plan, then commit, restore point first |
| `Destructive` | data loss or hard to reverse | plan, then commit, restore point first |

Properties enforced structurally:

1. Plan then commit. Anything not plainly allowed returns a plan and changes nothing.
   Applying needs an explicit second call with `commit` set true.
2. Fails closed. A missing tier rule, a malformed verdict, or an unknown capability id all
   deny. An unknown tier name in policy throws at load, so a typo cannot leave a tier
   unpoliced.
3. Policy cannot weaken the handshake. Omitting `dryRunOnly` for a System tier does not
   disable it, because `RequiresCommit` is derived from the tier.
4. No restore point, no commit. System tier and above refuse to commit when snapshots are
   unavailable, which is the default unelevated because VSS needs administrator rights.
   Capabilities a shadow copy cannot protect, such as killing a process, opt out explicitly
   with a stated reason.
5. Always audited. One entry per call including denials, unknown ids, and exceptions. A
   failed audit write fails the call.
6. Secrets never reach the log. Credential shaped keys are redacted, whole subtrees
   included.
7. Path guard canonicalizes before comparing, so traversal and prefix collisions cannot
   slip through.
