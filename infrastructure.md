# infrastructure

Holistic view of the system. Components, how they talk, and what moves between them.

## framing: this is a harness

AgenticOS is an agent harness in the harness engineering sense. The model reasons, the
harness acts, and the harness is what decides whether the thing is reliable. The central
tenet is already the shape of this codebase: the model never calls a tool directly. It
emits a structured call, and the harness validates the schema, checks permissions,
executes, and injects the result back. That is `CapabilityBroker`.

Mapping the twelve harness primitives against what exists tells us honestly where we are.

| primitive | state | where |
|---|---|---|
| tool design | done | one MCP tool per capability, typed schemas, explicit truncation flags |
| skills and MCP | done | three stdio servers, reused by Claude Code and Claude Desktop |
| permissions and authorization | done | risk tiers, allowed roots, command allowlist, all structured rather than prose |
| human in the loop | done | plan then commit handshake, kill switch |
| observability and tracing | done | append only JSONL, one entry per call including denials |
| verification | partial | post-condition checks on mutations, harness invariant suite |
| debugging and developer experience | partial | audit log, capability introspection tool |
| agent loop | missing | phase 2 |
| planning and task decomposition | missing | phase 2 |
| context delivery and compaction | missing | phase 2 |
| memory and state | missing | phase 2 |
| orchestration | missing | phase 2 |

The five that are done are the foundation ones, which is the right order. The gaps cluster
almost entirely in phase 2, with one exception. Verification was absent from the original
plan and is now first class, because it is the primitive that turns a demo into something
trustworthy.

### the rule we now build to

Mitchell Hashimoto's definition is the operating principle: any time the agent makes a
mistake, engineer a solution so it can never make that mistake again. Applied here, a bug
is not finished when it is fixed. It is finished when a permanent guard exists that would
have caught it.

Every bug found so far has been converted this way, which is why the list is worth keeping
in [progress-report.md](progress-report.md) rather than just in git history.

| mistake found | permanent guard now in place |
|---|---|
| int argument unreadable as long | `JsonArgs` coercion plus regression tests on both call paths |
| installed policy silently stale | module 20 syncs on hash, not on existence |
| server published under the wrong name | converge check in the provisioning runner |
| child process inherited the protocol pipe | stdin redirected and closed in `CommandRunner` |
| unbounded wait hung past the timeout | bounded drain wait |
| provisioning closure scoping | authoring rule documented at the runner top |
| stale exe against a newer shared DLL | staleness is now solution wide, plus a per server smoke test |

### verification, concretely

Two mechanisms, both borrowed from what already worked.

First, post-condition checks. The provisioning runner re-runs a step's `Test` after its
`Set` and fails when the state did not actually converge. That caught a real bug. Mutating
capabilities now do the same: after a committed change, the capability confirms the world
looks the way it claimed it would.

A failed check does not report `Failed`, because that would read as "nothing happened" and
invite a blind retry that applies the change twice. It reports `AppliedButUnverified`,
which tells the agent the mutation landed and the result needs checking by a human.

Second, harness invariants. A test suite asserts properties that must hold for every
capability that will ever be written, rather than for the ones that exist today. A dry run
never mutates. `System` and above always require a commit. Every call writes exactly one
audit entry. New capabilities are held to these without anyone remembering to add a test.

## component map

```
custom Windows image (phase 6)
  provisioning modules applied offline to a mounted WIM

  heads up display (phase 3)
    C sharp tray app: overlay, RegisterHotKey listener, one resident session
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
{ "status": "Succeeded | DryRun | Denied | Failed | AppliedButUnverified", "message": "...", "result": { } }
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
| `Write` | recoverable change to user data | plan, then commit, derived from the tier |
| `System` | machine or OS state | plan, then commit, restore point first |
| `Destructive` | data loss or hard to reverse | plan, then commit, restore point first |

Properties enforced structurally:

1. Plan then commit, checked against plans actually shown. Anything not plainly allowed
   returns a plan and changes nothing, and a commit must redeem a plan this process issued
   for the same capability and the same arguments. One plan authorises one commit, and it
   expires after ten minutes.

   This was wrong for a long time. The broker computed the dry-run flag purely from the
   commit flag and nothing remembered whether a plan had ever been produced, so a first-ever
   call with commit set true applied immediately. The default approver made it circular by
   answering yes whenever that same flag was set. The handshake was one call wearing the
   costume of two, and it is now a property of the harness rather than a convention the
   caller may follow.
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
