# progress report

Where the plan stands. Updated 2026 07 26.

## summary

Phase 0 complete. Three of four phase 1 servers done. Verification added as a new phase and
complete. Published to GitHub as a private repo.

| phase | target | status | note |
|---|---|---|---|
| 0 | 1 session | done | both acceptance criteria verified |
| 1 | 1 to 2 weeks | in progress | 3 of 4 servers, aos-apps blocked on Google credentials |
| 1v | 2 days | done | post-conditions and harness invariants |
| 2 to 7 | see planning.md | not started | |

## reframe

The project is now built to harness engineering rules, which the framing section in
[infrastructure.md](infrastructure.md) maps out. This was a refinement rather than a
rewrite, because the central tenet, that the model never calls a tool directly and the
harness validates and executes on its behalf, was already the shape of the codebase.

Five of the twelve harness primitives are done and they are the foundation ones. The gaps
sit almost entirely in phase 2, with verification the one genuine omission, now closed.

## what is verified working

Three servers published to `%LOCALAPPDATA%\AgenticOS\bin` and registered in Claude Code
through `.mcp.json` and in Claude Desktop through provisioning module 30.

| server | tools | exercised against |
|---|---|---|
| aos-windows | 9 | real windows, real control trees, a real process kill |
| aos-files | 9 | real Downloads contents, 172 files scanned by grep |
| aos-shell | 3 | real git commands, four refused boundary attempts |

An agent app ships as `aos.cmd`, built on the Claude Agent SDK against those servers over
stdio. Verified end to end: it listed the real open windows with correct focus in 7.9
seconds. `Ctrl+Alt+A` launches it from anywhere, through a Start Menu shortcut hotkey.

ReadyToRun cut server cold start, most sharply for the one that loads WPF.

| server | before | after |
|---|---|---|
| aos-windows | 2637 ms | 864 ms |
| aos-shell | 1559 ms | 891 ms |
| aos-files | 1348 ms | 1196 ms |

The before figures were single runs and the after figures are best of three, so treat the
direction as solid and the exact numbers as indicative. Cost is a 11.7 MB bin directory.

Checks that passed on live servers over real JSON RPC, not mocks.

| check | result |
|---|---|
| window and control tree reads | correct handles, pids, ref paths |
| System tier process kill | dry run left it alive, commit killed it |
| trash and restore round trip | file gone, then back, contents intact |
| allowed root boundary | System32 refused, message names the roots |
| shell allowlist | unlisted exe, path form, and bad working directory all refused |
| shell injection | a chained command passed as an argument stayed literal text |
| post-condition checks | move, trash, and restore all report verified success |
| test suite | 133 passing |
| provisioning | 26 ok, 0 changed on re apply |

## deadlines

Phase 1 is on track. `aos-apps` needs a Google Cloud OAuth client, which only the account
owner can create, so it is the one item that cannot be finished unattended.

Phase 6 prerequisites are half met. VMware Workstation is installed. Windows Hypervisor
Platform still needs enabling from an elevated shell, because WSL2 and Docker already hold
the hypervisor. The Windows ADK is still not installed. None of this blocks phases 1 to 5.

## mistakes and their permanent guards

The harness rule is that a bug is finished when a guard exists that would have caught it.

| mistake | permanent guard |
|---|---|
| int argument unreadable as long | `JsonArgs` coercion, regression tests on both call paths |
| installed policy silently stale | module 20 syncs on hash rather than existence |
| server published under the wrong name | converge check in the provisioning runner |
| child inherited the MCP protocol pipe | stdin redirected and closed in `CommandRunner` |
| unbounded wait hung past the timeout | bounded drain wait |
| provisioning closure scoping | authoring rule at the top of the runner |
| user name in test data | pre publish scan, now part of the release routine |
| stale exe against a newer shared DLL | staleness is solution wide, plus a smoke test per server |
| agent app hung on piped stdin | readline EOF ends the loop rather than being swallowed |
| closure read an inherited variable and saw null | authoring rule now names the inherited case, not just the module local one |
| shortcut hotkey compared by spelling | compared by normalised parts, since Windows rewrites the string |
| plan then commit was really one call | a plan ledger, so a commit must redeem a plan actually shown |
| cancelled calls wrote no audit entry at all | audited before rethrowing, and written with a token that cannot be cancelled |
| malformed policy verdict failed open | Enum.IsDefined plus default deny on anything not Allow or Prompt |
| Write tier handshake came from policy, not the tier | RequiresCommit derives from Write upward |
| extended length paths skipped canonicalisation | those forms refused, and reparse points resolved |
| Reason and Message reached the log verbatim | both scrubbed and truncated, plus value pattern matching |
| batch shims let cmd.exe re-parse arguments | .bat and .cmd refused, PATHEXT ignored |
| interpreters in the allowlist meant arbitrary code | removed from defaults, plus per command argument patterns |
| trash restore never consulted the path guard | guard injected into the trash store |
| a UIA ref could resolve to a different control | mutations pin an expected name or automation id |
| address fields could inject mail headers | control characters refused in to and cc |
| module 30 wrote a BOM and broke Claude Desktop | written with an explicit no BOM encoder |
| benign stderr aborted provisioning steps | stderr redirection removed from native calls |

The lower fourteen came from a deep quality check by three parallel review agents pointed at
the broker, the servers, and the provisioning and TypeScript code. Several invalidated claims
this project had been making confidently, which is the point of running the check at all.

Two earlier ones were found only by driving the live server, and both passed every unit test
while broken. That is the argument for JJtorio issue 6 in one line.

One verification of mine was itself flawed and had to be redone. The first process kill test
used `Start-Process notepad`, which returns a stub process id on Windows 11 because Notepad
is a Store application, and the liveness check reported alive for a process that never
existed. It would have passed while proving nothing.

## next

1. `aos-apps`. Gmail and Google Calendar over OAuth2. Needs a Google Cloud client id.
2. Phase 2 orchestrator, which is where the remaining harness primitives live: the agent
   loop, planning, context compaction, memory, and orchestration.
