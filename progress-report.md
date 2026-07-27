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
| negative `top` on process list | clamps to 1 instead of reporting no processes at all |
| capture of a minimized window | refused with a reason, rather than a black PNG called success |
| screen capture on a 150 percent display | 2400 by 1600, the true pixels, not 1600 by 1067 |
| control tree completeness flag | false on a tree that fits, true only when something was cut |
| control tree on a slow app | partial result plus a note at 15 seconds, rather than no answer |
| a wrong `expectTitle` on a real window | refused, and the message names both titles |
| a wrong `expectName` on a real process kill | refused at commit, Notepad still running afterwards |
| the same kill with the right name | plan, then commit, then the pid is genuinely gone |
| the daily brief, end to end | real repos, real issues, real Downloads, 9.8 seconds |
| tidy downloads, on the real folder | 134 loose files to 44, every one of the 134 accounted for |
| staged trash round trip | a trashed installer restored byte for byte to where it came from |
| purge of an aged entry | exactly 2 MB reclaimed, slot removed, other 51 entries untouched |
| purge age floor | refuses everything under the stated age, checked at the boundary |
| test suite | 164 C# and 23 TypeScript passing |
| provisioning | 30 ok, 0 changed on re apply |

The brief is the first part of this that pays for itself without being asked. This morning it
found 22 uncommitted files in one project, six branches with no upstream at all, a repo with
real work and no commits in it, and 14 GB of Downloads clutter going back 563 days.

`aos tidy-downloads` is the first part that acts. It reports 42 spent installers and archives
worth 6 GB, and files 48 keepers by kind. It refuses to touch folders, partial downloads, or
anything it does not recognise, and it leaves the 9.4 GB Factorio archive alone because at 46
days it is under the threshold. Nothing runs without `--commit`, and everything it trashes
comes back with one call.

The commit path was proved against a scratch folder of backdated fixtures, not against real
Downloads. That was the point: the first run had a bug that renamed files, and finding that
out on 90 of your own files would have been a poor way to learn it.

It has since run on the real folder, on request. 134 loose files became 44, and all 134 were
reconciled afterwards against a snapshot taken before: 44 still loose, 48 filed, 42 in trash,
nothing lost and nothing renamed.

Then the honest part. Nothing had been reclaimed. Staged trash lives on the same volume as
Downloads, so 6 GB had simply moved from one folder on C: to another, and 15 GB was sitting
there in total. A staged trash that never empties is a second Downloads folder, and the space
saving the report implied did not exist. That gap is now closed by `trash.purge`, and the brief
reports staged trash as pending rather than dealt with.

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
| a git failure read as a clean repo | dirty count is nullable, and the all clear names unchecked repos |
| a timeout came back as exit code 1 | `failureKind` separates killed from genuinely failed |
| a minimized window captured as a black PNG | refused, since the size guard could never catch it |
| process handles leaked once per call | every enumerated process disposed, filtered ones included |
| a negative `top` looked like an empty machine | clamped, as every sibling capability already was |
| a complete tree announced itself as truncated | the flag asks whether a child was actually cut off |
| a slow app hung the control tree read | a wall clock budget returns a partial tree and says so |
| coordinates disagreed between capture and UIA | per monitor DPI awareness declared before the first call |
| a torn token write lost the refresh token | written to a temporary file and moved into place |
| concurrent refreshes raced each other out | serialized, with the token re read inside the lock |
| a message id was interpolated into a URL path | escaped to one segment, and blank ids refused |
| trashing across drives threw a flat IO error | copy then delete, with the source removed only after |
| the dependency check probed one package | npm's own install marker compared against the manifests |
| a disabled or retimed task still read as ok | the test checks the trigger and the state, not existence |
| a publish step could never satisfy its own test | publish records a stamp instead of trusting timestamps |
| the ref guard existed and no tool could reach it | a reflection test reads the real signatures |
| a recycled handle could redirect an approved action | optional `expectTitle` on every hwnd taking tool |
| a recycled pid could redirect a kill | optional `expectName`, and the plan says what to pass |
| the TypeScript side had no tests at all | `node --test`, run by the converge, no new dependency |
| a query that was never meaningful read as a failure | repos with no commits skip the upstream check |
| a move renamed files and reported success | the caller checks the name, since the broker cannot |
| the brief named a command the launcher did not have | `aos.cmd` dispatches routines, and a test asserts the pointer |
| trashing was reported as reclaiming space | the brief says pending, and `trash.purge` actually reclaims |
| one file appeared three times in the trash list | one row per id, from its latest manifest line |
| a restore plan promised something already purged | the plan resolves the entry instead of echoing the id |
| a restored entry read as unaccountably missing | restores are recorded, so the four end states are distinct |
| trash age measured from file mtime, not deletion | read from the manifest, which is what purge keys off |

The middle fourteen came from a deep quality check by three parallel review agents pointed at
the broker, the servers, and the provisioning and TypeScript code. Several invalidated claims
this project had been making confidently, which is the point of running the check at all.

The last of them is the sharpest example of the rule working. The solution wide staleness
check was itself a fix for an earlier bug, and it was wrong: `dotnet publish` is content
incremental and the copy into bin preserves the source timestamp, so an executable whose own
project genuinely did not change was never re stamped and its test could never become true.
The converge reported three failures in a row that no amount of republishing would clear. A
step that cannot reach its own definition of done is a broken step, not a stale binary.

Two earlier ones were found only by driving the live server, and both passed every unit test
while broken. That is the argument for JJtorio issue 6 in one line.

One verification of mine was itself flawed and had to be redone. The first process kill test
used `Start-Process notepad`, which returns a stub process id on Windows 11 because Notepad
is a Store application, and the liveness check reported alive for a process that never
existed. It would have passed while proving nothing.

Driving live servers is now a script rather than a habit. `provisioning\Invoke-AosTool.ps1`
speaks MCP to a published server and calls one tool, which is how the behaviour changes in
the table above were confirmed against real windows rather than against my expectations.
Most of them would have passed every unit test while broken.

It also does plan then commit over one connection, which is not a convenience. The plan
ledger lives in the server process, so a plan and a commit issued as two separate probe runs
can never match, and my first attempt at testing the pid guard only ever reached the ledger's
refusal. Testing a mutating capability one call per process proves nothing about the
capability.

The newest routine had a bug on its very first commit run, and it is a good illustration of
why a post-condition in the broker is not the same as a post-condition in the caller.
`files_move` moves an item into a destination only when that destination is an existing
folder; otherwise the destination becomes the item's new path. Passing the bare bucket folder
therefore turned `report.pdf` into a file named `documents` with no extension, and the broker's
own check passed, because it asks whether something arrived at the destination and something
had. Only the caller knew the file was supposed to keep its name. The guard now lives there,
and it refuses the move rather than reporting a rename as a success.

Had that run against the real Downloads folder instead of a scratch folder of backdated
fixtures, it would have mangled the first document of each kind and failed the other 46 on
collision, all under the heading "applied cleanly".

The worst find in the previous round was a guard that could not fire. `UiaSurface` read `expectName`
and `expectAutomationId`, the comment explained exactly which attack they stopped, and no tool
method exposed either parameter, so nothing could ever supply one. The code read as protected
and was not. The permanent guard is a test that reflects over the real tool signatures, and it
immediately caught a second case I had missed while writing it.

## next

1. The 15 GB in staged trash becomes purgeable from late August, thirty days after it went in.
   Until then it is deliberately stuck, which is the price of every trash being reversible.
2. `aos-apps`. Gmail and Google Calendar over OAuth2. Needs a Google Cloud client id.
3. The six JJtorio employee branches with no upstream. The brief flags them every morning and
   nothing yet pushes them, which is the next routine worth writing.
4. Phase 2 proper: memory, model routing, and a scheduler for more than one routine.
