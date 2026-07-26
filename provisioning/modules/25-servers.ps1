# Publishes the MCP capability servers into %LOCALAPPDATA%\AgenticOS\bin.
#
# STALENESS IS SOLUTION-WIDE, NOT PER PROJECT.
#
# All servers publish into one shared bin directory and share Aos.Core, Aos.Broker and
# Aos.Mcp.Shared. That sharing is deliberate, and it means a server can go stale without
# any file in its own project folder changing.
#
# An earlier version of this module compared each exe only against sources under its own
# project directory. Adding a parameter to a method in Aos.Mcp.Shared therefore did not
# mark aos-windows or aos-shell as stale, but publishing aos-files overwrote the shared
# DLL in bin with the new signature. Both older exes then died at startup with
# MissingMethodException, which surfaced to a client as servers that connected and
# registered no tools. A per-project freshness check is simply wrong for binaries that
# share a folder and share dependencies.
#
# So: one timestamp for the newest source anywhere under src, compared against every exe.
# A shared-dependency change now republishes the whole set.
#
# Offline image builds (Phase 6) ship the published output instead of running dotnet,
# since the SDK is not present in a WIM.

$binDir = Join-Path $AosRoot 'bin'

function global:Get-AosNewestSourceUtc {
    param([Parameter(Mandatory)][string] $SourceRoot)

    if (-not (Test-Path -LiteralPath $SourceRoot)) { return [DateTime]::MinValue }

    $newest = Get-ChildItem -Path $SourceRoot -Recurse -File -Include '*.cs', '*.csproj' -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -notmatch '\\(obj|bin)\\' } |
        Sort-Object LastWriteTimeUtc -Descending |
        Select-Object -First 1

    if ($newest) { return $newest.LastWriteTimeUtc }
    [DateTime]::MinValue
}

function New-AosPublishStep {
    param(
        [Parameter(Mandatory)][string] $ProjectDir,
        [Parameter(Mandatory)][string] $ExeName
    )

    $project    = Join-Path $RepoRoot $ProjectDir
    $exe        = Join-Path $binDir $ExeName
    $target     = $binDir
    $sourceRoot = Join-Path $RepoRoot 'src'

    New-AosStep -Name "publish: $ExeName" -Test {
        if (-not (Test-Path -LiteralPath $exe)) { return $false }
        if (-not (Test-Path -LiteralPath $project)) {
            # Nothing to build from (offline image build); a staged exe is acceptable.
            return $true
        }

        (Get-Item -LiteralPath $exe).LastWriteTimeUtc -ge (Get-AosNewestSourceUtc -SourceRoot $sourceRoot)
    }.GetNewClosure() -Set {
        if (-not (Test-Path -LiteralPath $project)) {
            throw "Cannot publish: project '$project' is missing and no prebuilt exe was staged."
        }
        New-Item -ItemType Directory -Force -Path $target | Out-Null
        $output = & dotnet publish $project -c Release -o $target --nologo
        if ($LASTEXITCODE -ne 0) { throw "dotnet publish failed:`n$($output -join "`n")" }
    }.GetNewClosure()
}

# Smoke test: does the exe actually start, speak MCP, and list tools?
#
# A server that launches and registers nothing looks identical to a healthy one from the
# client's side, so the only way to catch it here is to ask. This is the permanent guard
# for the stale-binary class of failure, independent of the freshness check above.
function New-AosServerSmokeStep {
    param([Parameter(Mandatory)][string] $ExeName)

    $exe  = Join-Path $binDir $ExeName
    $name = $ExeName

    New-AosStep -Name "smoke: $name lists tools" -Test {
        if (-not (Test-Path -LiteralPath $exe)) { return $false }

        $psi = New-Object Diagnostics.ProcessStartInfo
        $psi.FileName = $exe
        $psi.RedirectStandardInput = $true
        $psi.RedirectStandardOutput = $true
        $psi.RedirectStandardError = $true
        $psi.UseShellExecute = $false

        $proc = $null
        try {
            $proc = [Diagnostics.Process]::Start($psi)

            # stderr must be drained. It is redirected, so a server that logs more than the
            # pipe buffer holds would block writing to it, never answer on stdout, and burn
            # both timeouts below on an otherwise healthy build.
            $stderrTask = $proc.StandardError.ReadToEndAsync()

            $proc.StandardInput.WriteLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"aos-smoke","version":"1"}}}')
            $proc.StandardInput.Flush()

            $initTask = $proc.StandardOutput.ReadLineAsync()
            if (-not $initTask.Wait(20000)) { return $false }
            if (-not $initTask.Result) { return $false }

            $proc.StandardInput.WriteLine('{"jsonrpc":"2.0","method":"notifications/initialized"}')
            $proc.StandardInput.WriteLine('{"jsonrpc":"2.0","id":2,"method":"tools/list"}')
            $proc.StandardInput.Flush()

            # Read until the tools/list reply arrives. A server may interleave log
            # notifications, and matching a regex against one arbitrary line would accept
            # any of them, or even the serverInfo name from the initialize reply. That would
            # pass a server listing zero tools, which is the exact failure this step exists
            # to catch, so the reply is parsed and the tool count actually checked.
            for ($attempt = 0; $attempt -lt 10; $attempt++) {
                $lineTask = $proc.StandardOutput.ReadLineAsync()
                if (-not $lineTask.Wait(20000)) { return $false }
                $line = $lineTask.Result
                if (-not $line) { return $false }

                try { $parsed = $line | ConvertFrom-Json } catch { continue }
                if ($parsed.id -ne 2) { continue }

                return [bool]($parsed.result -and $parsed.result.tools -and @($parsed.result.tools).Count -gt 0)
            }

            return $false
        }
        catch {
            return $false
        }
        finally {
            if ($proc) {
                try { $proc.StandardInput.Close() } catch { }
                try {
                    if (-not $proc.WaitForExit(5000)) {
                        # Kill the tree: aos-shell can have children of its own, and killing
                        # only the parent orphans them holding the published DLLs, which then
                        # blocks the next publish.
                        $proc.Kill($true)
                        $proc.WaitForExit(2000) | Out-Null
                    }
                } catch { }
                $proc.Dispose()
            }
        }
    }.GetNewClosure() -Set {
        throw "'$name' started but listed no tools. Run it directly to see stderr. A MissingMethodException there means a stale exe against a newer shared DLL."
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

    New-AosPublishStep -ProjectDir 'src\Aos.Mcp.Shell'   -ExeName 'aos-mcp-shell.exe'

    New-AosServerSmokeStep -ExeName 'aos-mcp-windows.exe'
    New-AosServerSmokeStep -ExeName 'aos-mcp-files.exe'
    New-AosServerSmokeStep -ExeName 'aos-mcp-shell.exe'
)
