# progress report

Where the plan stands. Updated 2026 07 25.

## summary

Phase 0 complete. Phase 1 is roughly one third done. Three commits. On schedule.

| phase | target | status | note |
|---|---|---|---|
| 0 | 1 session | done | both acceptance criteria verified |
| 1 | 1 to 2 weeks | in progress | broker plus 1 of 4 servers |
| 2 to 7 | see planning.md | not started | |

## what is verified working

`aos-windows` is published to `%LOCALAPPDATA%\AgenticOS\bin` and registered in Claude Code
through `.mcp.json` and in Claude Desktop through provisioning module 30. Nine tools over
eight brokered capabilities.

Verified against a live server over real JSON RPC on stdio, not mocks.

| check | result |
|---|---|
| `window_list` | returned the real editor window with correct handle and pid |
| `ui_tree` | walked the real control tree with ref paths and per element actions |
| `screen_capture` | wrote a 2.3 MB PNG |
| System tier kill | dry run named the process and left it alive, commit killed it |
| audit log | both calls recorded with arguments and reason |
| test suite | 63 passing |
| provisioning | 14 ok, 0 changed on re apply |

## deadlines

Phase 1 is on track. No dates need to change yet.

Phase 6 has an unfunded prerequisite. The Windows ADK is not installed, and Windows 11 Home
has no Hyper V, so a third party virtual machine host is needed. Neither blocks phases 1
through 5, but both should be set up before phase 6 starts so the estimate holds.

## what was learned

Two real bugs, both found by driving the live server rather than trusting unit tests.

1. `JsonValue.GetValue<long>()` requires an exact type match for nodes built in process, so
   a value constructed from an `int` throws, while the same argument arriving over the wire
   is JsonElement backed and converts freely. This would have broken every `int` typed tool
   parameter. Coercion now lives in `Aos.Core.JsonArgs`, shared by all servers, with
   regression tests for both paths.
2. A scoping conflict in provisioning. `GetNewClosure` fixes variable capture but snapshots
   the module scope, so runner scope functions stop resolving inside closures. Shared
   helpers are now global and removed on exit. Both rules are documented at the top of
   `Install-Aos.ps1`, because getting either wrong fails at step execution time rather than
   parse time.

One flawed verification, caught and redone. The first process kill test used
`Start-Process notepad`, which returns a stub process id on Windows 11 because Notepad is a
Store application, and the liveness check reported alive for a process that did not exist.
Redone with a process under full control.

One design call worth repeating. `process.stop` opts out of the restore point requirement
with a stated reason, because a shadow copy cannot un kill a process and demanding one would
block the capability for no safety gain. The commit handshake still applies. Future
capabilities should opt out with a reason rather than weakening the tier.

## next

1. `aos-files`. Content search, USN journal recent activity, safe organize into staged
   trash. This is the server that makes filing work.
2. `aos-apps`. Gmail and Google Calendar over OAuth2.
3. `aos-shell`. Policy gated PowerShell, with raw shell staying denied.
