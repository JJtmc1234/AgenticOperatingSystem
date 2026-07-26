# Global hotkey to launch the agent.
#
# Uses a Start Menu shortcut with its Hotkey property set, rather than a resident process
# calling RegisterHotKey. Explorer already services shortcut hotkeys system wide, so this
# needs nothing running in the background and nothing to keep alive across reboots.
#
# The tradeoffs, honestly: Explorer's dispatch has a noticeable delay compared with a
# resident listener, the shortcut has to live in Start Menu or on the Desktop for the key
# to register at all, and the combination can be claimed by another app. The Phase 3
# display replaces this with a real RegisterHotKey listener and a persistent session, which
# also removes the per launch cold start. Until then this is the cheap 90 percent.

$hotkey       = 'CTRL+ALT+A'
# Copied into module-local variables on purpose. GetNewClosure captures variables that
# exist in *this* scope; $RepoRoot is inherited from the runner, so a closure that reads it
# directly gets nothing. That is how WorkingDirectory silently came out empty.
$repoRoot     = $RepoRoot
$launcher     = Join-Path $repoRoot 'aos.cmd'
$startMenu    = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs'
$shortcutPath = Join-Path $startMenu 'AgenticOS Agent.lnk'

# WScript.Shell rewrites the Hotkey string it is given: 'CTRL+ALT+A' reads back as
# 'Alt+Ctrl+A'. Comparing the raw strings therefore always fails, and the step would
# rewrite the shortcut on every run. Compare the parts instead of the spelling.
function global:Test-AosHotkeyMatch {
    param(
        [Parameter(Mandatory)][AllowEmptyString()][string] $Actual,
        [Parameter(Mandatory)][string] $Expected
    )

    $normalize = {
        param($value)
        if ([string]::IsNullOrWhiteSpace($value)) { return '' }
        ($value.Split('+') |
            ForEach-Object { $_.Trim().ToUpperInvariant() } |
            Where-Object { $_.Length -gt 0 } |
            Sort-Object) -join '+'
    }

    (& $normalize $Actual) -eq (& $normalize $Expected)
}

@(
    New-AosStep -Name 'launcher exists (aos.cmd)' -Test {
        Test-Path -LiteralPath $launcher -PathType Leaf
    }.GetNewClosure()

    New-AosStep -Name "hotkey: $hotkey launches the agent" -Test {
        if (-not (Test-Path -LiteralPath $shortcutPath)) { return $false }

        $shell = New-Object -ComObject WScript.Shell
        try {
            $link = $shell.CreateShortcut($shortcutPath)
            # All three must match, or the shortcut is stale from an earlier checkout,
            # a different key, or a run where the working directory did not stick.
            ($link.TargetPath -eq $launcher) -and
            ($link.WorkingDirectory -eq $repoRoot) -and
            (Test-AosHotkeyMatch -Actual $link.Hotkey -Expected $hotkey)
        }
        finally {
            [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell)
        }
    }.GetNewClosure() -Set {
        if (-not (Test-Path -LiteralPath $startMenu)) {
            throw "Start Menu programs folder not found at '$startMenu'."
        }

        $shell = New-Object -ComObject WScript.Shell
        try {
            $link = $shell.CreateShortcut($shortcutPath)
            $link.TargetPath = $launcher
            $link.WorkingDirectory = $repoRoot
            $link.Description = 'Launch the AgenticOS agent'
            $link.Hotkey = $hotkey
            $link.WindowStyle = 1
            $link.Save()
        }
        finally {
            [void][Runtime.InteropServices.Marshal]::ReleaseComObject($shell)
        }
    }.GetNewClosure()
)
