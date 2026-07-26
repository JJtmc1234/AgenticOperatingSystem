# Installs the capability policy the broker reads at startup.
#
# The repo file is the source of truth and the live copy is synced whenever it differs.
# An earlier version of this module only checked that the live file existed, so adding
# allowedRoots to the repo policy never reached the installed copy and every file
# capability failed with "no allowed root exists". Existence is not a strong enough test
# for a file whose contents matter.
#
# Local tightening therefore belongs in the repo file, where it is version controlled,
# rather than as an untracked edit under %LOCALAPPDATA% that silently diverges.

$sourcePolicy = Join-Path $RepoRoot 'policy\default.yaml'
$livePolicy   = Join-Path $AosRoot  'policy.yaml'

@(
    New-AosStep -Name "policy source exists: $sourcePolicy" `
        -Test { Test-Path -LiteralPath $sourcePolicy -PathType Leaf }.GetNewClosure()

    New-AosStep -Name "policy in sync: $livePolicy" -Test {
        if (-not (Test-Path -LiteralPath $livePolicy -PathType Leaf)) { return $false }

        $sourceHash = (Get-FileHash -LiteralPath $sourcePolicy -Algorithm SHA256).Hash
        $liveHash   = (Get-FileHash -LiteralPath $livePolicy   -Algorithm SHA256).Hash
        $sourceHash -eq $liveHash
    }.GetNewClosure() -Set {
        Copy-Item -LiteralPath $sourcePolicy -Destination $livePolicy -Force
    }.GetNewClosure()
)

# Modules 30+ register the published servers with the MCP clients.
