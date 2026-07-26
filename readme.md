# agenticos

An agent native layer over Windows, built to be baked into a custom Windows image.

Guiding decision: the custom image is a packaging target, not a development environment.
Everything is an idempotent provisioning module that installs onto a live machine and gets
used daily. The image build applies those same tested modules offline to a mounted WIM,
which keeps it cheap.

## docs

| file | contents |
|---|---|
| [brainstorm.md](brainstorm.md) | how the idea was reached and why the image is a packaging target |
| [planning.md](planning.md) | the seven phases, effort, and acceptance criteria |
| [infrastructure.md](infrastructure.md) | components, transports, data flow, safety model |
| [progress-report.md](progress-report.md) | current status, deadlines, what was learned |

## getting started

```powershell
dotnet build aos.sln
dotnet test aos.sln
.\provisioning\Install-Aos.ps1 -WhatIf   # report what would change
.\provisioning\Install-Aos.ps1           # publish servers, install policy, register MCP
```

Runtime state lives in `%LOCALAPPDATA%\AgenticOS` and is never committed.

Claude Code reads the servers from `.mcp.json` in this repo. Claude Desktop is registered by
`provisioning/modules/30-mcp.ps1`, which merges into its config and backs it up first.

## gotchas worth knowing before you edit

Provisioning module scoping. Steps are collected from a module, then invoked after that
module scope is gone. So module local variables referenced by a step must be captured with
`.GetNewClosure()`. Because `GetNewClosure` snapshots the module scope, runner scope
functions would not resolve from inside a closure either, so shared helpers are defined
global and removed on exit. Both rules sit at the top of `provisioning/Install-Aos.ps1`.
Getting either wrong fails at step execution time, not parse time.

`System.IO` is not in the implicit using set for WindowsDesktop SDK projects that set
`UseWPF` or `UseWindowsForms`, though it is for plain `net9.0`. Import it explicitly.

`JsonValue.GetValue<T>()` demands an exact type match for nodes built in process, so a value
constructed from an `int` throws when read as a `long`, while the same argument arriving over
the wire is JsonElement backed and converts freely. Always go through `Aos.Core.JsonArgs`.

UIAutomation refs. `ui.tree` addresses elements by a path of child indices such as `0.3.1`
rather than display name, because names repeat, localize, and contain whitespace. Refs go
stale when the interface changes, and the error says so.

## house rules for this repo

Filenames use lower case only. Documentation is `.md` only. Prose avoids dashes and
semicolons. No local paths containing a user name anywhere in the repo, including tests.
Always quality check generated output before committing it.
