# Registers the capability servers with Claude Desktop.
#
# Claude Code picks the servers up from the repo's .mcp.json (project-scoped, committed),
# so only Claude Desktop needs its user config touched. Existing entries for other servers
# are preserved -- this merges, it never rewrites the file wholesale.

$desktopConfig = Join-Path $env:APPDATA 'Claude\claude_desktop_config.json'
$serverExe     = Join-Path $AosRoot 'bin\aos-mcp-windows.exe'
$serverKey     = 'aos-windows'

@(
    New-AosStep -Name "mcp: $serverKey registered in Claude Desktop" -Test {
        if (-not (Test-Path -LiteralPath $desktopConfig)) { return $false }

        $json = Get-Content -LiteralPath $desktopConfig -Raw | ConvertFrom-Json
        if (-not (Test-AosProperty $json 'mcpServers')) { return $false }
        if (-not (Test-AosProperty $json.mcpServers $serverKey)) { return $false }

        $json.mcpServers.$serverKey.command -eq $serverExe
    }.GetNewClosure() -Set {
        if (-not (Test-Path -LiteralPath $serverExe)) {
            throw "Server not published at '$serverExe'. Module 25-servers must run first."
        }
        if (-not (Test-Path -LiteralPath $desktopConfig)) {
            throw "Claude Desktop config not found at '$desktopConfig'."
        }

        # Back up before touching a config we do not own.
        $backup = "$desktopConfig.aos-backup"
        if (-not (Test-Path -LiteralPath $backup)) {
            Copy-Item -LiteralPath $desktopConfig -Destination $backup
        }

        $json = Get-Content -LiteralPath $desktopConfig -Raw | ConvertFrom-Json

        if (-not (Test-AosProperty $json 'mcpServers')) {
            $json | Add-Member -NotePropertyName 'mcpServers' -NotePropertyValue ([pscustomobject]@{})
        }

        $entry = [pscustomobject]@{ command = $serverExe; args = @() }

        if (Test-AosProperty $json.mcpServers $serverKey) {
            $json.mcpServers.$serverKey = $entry
        }
        else {
            $json.mcpServers | Add-Member -NotePropertyName $serverKey -NotePropertyValue $entry
        }

        $json | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $desktopConfig -Encoding utf8
    }.GetNewClosure()
)
