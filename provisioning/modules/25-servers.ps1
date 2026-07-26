# Publishes the MCP capability servers into %LOCALAPPDATA%\AgenticOS\bin.
#
# The Test compares the published exe against the newest source file rather than merely
# checking existence, so the step stays idempotent while still republishing after a code
# change. Offline image builds (Phase 6) ship the published output instead of running
# dotnet, since the SDK is not present in a WIM.

$binDir = Join-Path $AosRoot 'bin'

function New-AosPublishStep {
    param(
        [Parameter(Mandatory)][string] $ProjectDir,
        [Parameter(Mandatory)][string] $ExeName
    )

    $project = Join-Path $RepoRoot $ProjectDir
    $exe     = Join-Path $binDir $ExeName
    $target  = $binDir

    New-AosStep -Name "publish: $ExeName" -Test {
        if (-not (Test-Path -LiteralPath $exe)) { return $false }
        if (-not (Test-Path -LiteralPath $project)) {
            # Nothing to build from (image build); an existing exe is acceptable.
            return $true
        }

        $newestSource = Get-ChildItem -Path $project -Recurse -File -Include '*.cs', '*.csproj' |
            Where-Object { $_.FullName -notmatch '\\(obj|bin)\\' } |
            Sort-Object LastWriteTimeUtc -Descending |
            Select-Object -First 1

        if (-not $newestSource) { return $true }
        (Get-Item -LiteralPath $exe).LastWriteTimeUtc -ge $newestSource.LastWriteTimeUtc
    }.GetNewClosure() -Set {
        if (-not (Test-Path -LiteralPath $project)) {
            throw "Cannot publish: project '$project' is missing and no prebuilt exe was staged."
        }
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        $output = & dotnet publish $project -c Release -o $target --nologo 2>&1
        if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed:`n$($output -join "`n")" }
    }.GetNewClosure()
}

@(
    New-AosStep -Name "dir: $binDir (published servers)" -Test {
        Test-Path -LiteralPath $binDir -PathType Container
    }.GetNewClosure() -Set {
        New-Item -ItemType Directory -Force -Path $binDir | Out-Null
    }.GetNewClosure()

    New-AosPublishStep -ProjectDir 'src\Aos.Mcp.Windows' -ExeName 'aos-mcp-windows.exe'

    New-AosPublishStep -ProjectDir 'src\Aos.Mcp.Files'   -ExeName 'aos-mcp-files.exe'
)
