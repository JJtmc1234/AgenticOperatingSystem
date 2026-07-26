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

        # WriteAllText with an explicit no-BOM encoder, NOT Set-Content -Encoding utf8.
        #
        # On PowerShell 5.1, -Encoding utf8 emits a byte order mark. Claude Desktop is
        # Electron, and Node's JSON.parse rejects a leading BOM outright, so the first
        # successful run of this module silently disabled every MCP server in the user's
        # config, including ones we did not add. Provisioning could not see it either:
        # Get-Content -Raw strips the BOM on read, so the Test kept passing while the file
        # on disk was unparseable. A Test that is true while the desired state is false is
        # the worst kind.
        $text = $json | ConvertTo-Json -Depth 12
        [IO.File]::WriteAllText($config, $text, (New-Object Text.UTF8Encoding($false)))
    }.GetNewClosure()
}

@(
    New-AosMcpStep -ServerKey 'aos-windows' -ExeName 'aos-mcp-windows.exe'
    New-AosMcpStep -ServerKey 'aos-files'   -ExeName 'aos-mcp-files.exe'
    New-AosMcpStep -ServerKey 'aos-shell'   -ExeName 'aos-mcp-shell.exe'
    New-AosMcpStep -ServerKey 'aos-apps'    -ExeName 'aos-mcp-apps.exe'
)
