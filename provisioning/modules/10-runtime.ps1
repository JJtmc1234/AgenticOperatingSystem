# Runtime state directories under %LOCALAPPDATA%\AgenticOS. Kept out of the repo so the
# audit log and staged trash survive a clean clone and are never committed.

# Each call gets a fresh scope, and GetNewClosure binds $target into the scriptblocks.
# Building these in a ForEach-Object instead would leave every step pointing at the last
# directory, since the iterations share one scope.
function New-AosDirStep {
    param(
        [Parameter(Mandatory)][string] $Path,
        [Parameter(Mandatory)][string] $Note
    )
    $target = $Path
    New-AosStep -Name "dir: $target ($Note)" `
        -Test { Test-Path -LiteralPath $target -PathType Container }.GetNewClosure() `
        -Set  { New-Item -ItemType Directory -Force -Path $target | Out-Null }.GetNewClosure()
}

@(
    New-AosDirStep -Path $AosRoot                          -Note 'root'
    New-AosDirStep -Path (Join-Path $AosRoot 'audit')      -Note 'append-only capability audit (JSONL)'
    New-AosDirStep -Path (Join-Path $AosRoot 'trash')      -Note 'staged deletes -- destructive ops move here'
    New-AosDirStep -Path (Join-Path $AosRoot 'data')       -Note 'agent memory (SQLite)'
    New-AosDirStep -Path (Join-Path $AosRoot 'logs')       -Note 'service and orchestrator logs'
)
