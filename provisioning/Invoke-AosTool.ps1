# Drives a published MCP server over stdio and prints one tool's result.
#
# Exists because the only trustworthy check on a capability is the one that runs it. Unit
# tests cover the broker, but they cannot catch a JSON shape the real protocol rejects, a
# handler that throws only against live Windows state, or a server that starts and lists
# nothing. Every fix to a capability gets driven through here before it is called done.
#
#   .\Invoke-AosTool.ps1 -Server aos-mcp-windows.exe -Tool process_list `
#       -Arguments @{ top = -5 }
#
# -PlanThenCommit sends the same call twice, without commit and then with it, over ONE
# connection. That is not a convenience. The broker's plan ledger lives in the server
# process, so a plan and a commit issued from two separate probe runs can never match, and
# testing a mutating capability one call per process only ever exercises the refusal.
#
#   .\Invoke-AosTool.ps1 -Server aos-mcp-windows.exe -Tool process_stop `
#       -Arguments @{ pid = 1234; expectName = 'Notepad' } -PlanThenCommit

[CmdletBinding()]
param(
    [Parameter(Mandatory)][string] $Server,
    [Parameter(Mandatory)][string] $Tool,
    [hashtable] $Arguments = @{},
    [switch] $PlanThenCommit,
    [int] $TimeoutMs = 30000
)

$ErrorActionPreference = 'Stop'

$exe = Join-Path (Join-Path $env:LOCALAPPDATA 'AgenticOS\bin') $Server
if (-not (Test-Path -LiteralPath $exe)) { throw "No such server: $exe" }

$psi = New-Object Diagnostics.ProcessStartInfo
$psi.FileName = $exe
$psi.RedirectStandardInput = $true
$psi.RedirectStandardOutput = $true
$psi.RedirectStandardError = $true
$psi.UseShellExecute = $false

$proc = [Diagnostics.Process]::Start($psi)

try {
    # Redirected stderr must be drained or a chatty server blocks writing to a full pipe.
    $stderrTask = $proc.StandardError.ReadToEndAsync()

    $proc.StandardInput.WriteLine('{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"aos-probe","version":"1"}}}')
    $proc.StandardInput.Flush()

    $initTask = $proc.StandardOutput.ReadLineAsync()
    if (-not $initTask.Wait($TimeoutMs)) { throw 'Server did not answer initialize.' }

    $proc.StandardInput.WriteLine('{"jsonrpc":"2.0","method":"notifications/initialized"}')

    function Send-Call {
        param([int] $Id, [hashtable] $CallArguments)

        $call = @{
            jsonrpc = '2.0'
            id      = $Id
            method  = 'tools/call'
            params  = @{ name = $Tool; arguments = $CallArguments }
        } | ConvertTo-Json -Depth 12 -Compress

        $proc.StandardInput.WriteLine($call)
        $proc.StandardInput.Flush()

        # Log notifications can interleave, so the reply is matched by id rather than by
        # taking whatever line arrives first.
        for ($attempt = 0; $attempt -lt 20; $attempt++) {
            $lineTask = $proc.StandardOutput.ReadLineAsync()
            if (-not $lineTask.Wait($TimeoutMs)) { throw 'Timed out waiting for the tool result.' }
            $line = $lineTask.Result
            if (-not $line) { throw 'Server closed stdout before replying.' }

            try { $parsed = $line | ConvertFrom-Json } catch { continue }
            if ($parsed.id -ne $Id) { continue }

            if ($parsed.error) { throw "Server returned an error: $($parsed.error | ConvertTo-Json -Depth 6)" }

            foreach ($item in @($parsed.result.content)) {
                if ($item.type -eq 'text') { return $item.text }
            }
            return $null
        }

        throw 'No reply with the expected id arrived.'
    }

    if (-not $PlanThenCommit) {
        Write-Output (Send-Call -Id 2 -CallArguments $Arguments)
        return
    }

    # The plan and the commit must carry identical arguments apart from the flag, or the
    # ledger fingerprint will not match and the refusal tells you nothing about the
    # capability under test.
    $planArgs = @{} + $Arguments
    $planArgs.Remove('commit')

    $commitArgs = @{} + $planArgs
    $commitArgs['commit'] = $true

    Write-Output "--- plan ---"
    Write-Output (Send-Call -Id 2 -CallArguments $planArgs)
    Write-Output "--- commit ---"
    Write-Output (Send-Call -Id 3 -CallArguments $commitArgs)
}
finally {
    try { $proc.StandardInput.Close() } catch { }
    try {
        if (-not $proc.WaitForExit(5000)) {
            $proc.Kill($true)
            $proc.WaitForExit(2000) | Out-Null
        }
    } catch { }

    if ($VerbosePreference -eq 'Continue' -and $stderrTask) {
        try { Write-Verbose $stderrTask.Result } catch { }
    }
    $proc.Dispose()
}
