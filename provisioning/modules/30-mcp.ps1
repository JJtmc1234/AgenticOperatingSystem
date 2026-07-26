# Registers the capability servers with Claude Desktop.
#
# Claude Code picks the servers up from the repo's .mcp.json (project-scoped, committed),
# so only Claude Desktop needs its user config touched. Existing entries for other servers
# are preserved, this merges rather than rewriting the file wholesale.

$desktopConfig = Join-Path $env:APPDATA 'Claude\claude_desktop_config.json'

function New-AosMcpStep {
    param(
        [Parameter(Mandatory)][string] $ServerKey,
        [Parameter(Mandatory)][string] $ExeName
    )

    $serverExe = Join-Path $AosRoot "bin\$ExeName"
    $key       = $ServerKey
    $config    = $desktopConfig

    New-AosStep -Name "mcp: $key registered in Claude Desktop" -Test {
        if (-not (Test-Path -LiteralPath $config)) { return $false }

        $json = Get-Content -LiteralPath $config -Raw | ConvertFrom-Json
        if (-not (Test-AosProperty $json 'mcpServers')) { return $false }
        if (-not (Test-AosProperty $json.mcpServers $key)) { return $false }

        $json.mcpServers.$key.command -eq $serverExe
    }.GetNewClosure() -Set {
        if (-not (Test-Path -LiteralPath $serverExe)) {
            throw "Server not published at '$serverExe'. Module 25-servers must run first."
        }
        if (-not (Test-Path -LiteralPath $config)) {
            throw "Claude Desktop config not found at '$config'."
        }

        # Back up before touching a config we do not own.
        $backup = "$config.aos-backup"
        if (-not (Test-Path -LiteralPath $backup)) {
            Copy-Item -LiteralPath $config -Destination $backup
        }

        $json = Get-Content -LiteralPath $config -Raw | ConvertFrom-Json

        if (-not (Test-AosProperty $json 'mcpServers')) {
            $json | Add-Member -NotePropertyName 'mcpServers' -NotePropertyValue ([pscustomobject]@{})
        }

        $entry = [pscustomobject]@{ command = $serverExe; args = @() }

        if (Test-AosProperty $json.mcpServers $key) {
            $json.mcpServers.$key = $entry
        }
        else {
            $json.mcpServers | Add-Member -NotePropertyName $key -NotePropertyValue $entry
        }

        $json | ConvertTo-Json -Depth 12 | Set-Content -LiteralPath $config -Encoding utf8
    }.GetNewClosure()
}

@(
    New-AosMcpStep -ServerKey 'aos-windows' -ExeName 'aos-mcp-windows.exe'
    New-AosMcpStep -ServerKey 'aos-files'   -ExeName 'aos-mcp-files.exe'
)
