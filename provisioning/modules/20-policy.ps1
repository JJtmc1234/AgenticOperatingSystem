# Installs the capability policy the broker reads at startup.
#
# The live copy is deliberately NOT a symlink to the repo: local tightening should not
# show up as a dirty working tree, and an image build must bake a fixed policy. Repo
# changes are pulled forward explicitly by deleting the live copy and re-running.

$sourcePolicy = Join-Path $RepoRoot 'policy\default.yaml'
$livePolicy   = Join-Path $AosRoot  'policy.yaml'

@(
    New-AosStep -Name "policy source exists: $sourcePolicy" `
        -Test { Test-Path -LiteralPath $sourcePolicy -PathType Leaf }.GetNewClosure()

    New-AosStep -Name "policy installed: $livePolicy" `
        -Test { Test-Path -LiteralPath $livePolicy -PathType Leaf }.GetNewClosure() `
        -Set  { Copy-Item -LiteralPath $sourcePolicy -Destination $livePolicy }.GetNewClosure()
)

# Modules 30+ (MCP server registration, service install) land in Phase 1 and Phase 4.
