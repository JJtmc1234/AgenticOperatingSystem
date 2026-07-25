# Toolchain the rest of the modules assume. Detection only -- these steps deliberately
# have no Set, because silently installing SDKs behind the user's back is worse than
# failing loudly with a name to install.
#
# Test-AosCommand comes from Install-Aos.ps1: functions resolve at invocation time, so
# they need no closure. Module-local *variables* would (see the authoring rule there).

@(
    New-AosStep -Name '.NET SDK 9.x present' -Test {
        if (-not (Test-AosCommand 'dotnet')) { return $false }
        [bool](& dotnet --list-sdks | Where-Object { $_ -match '^9\.' })
    }

    New-AosStep -Name 'Node.js 22+ present' -Test {
        if (-not (Test-AosCommand 'node')) { return $false }
        $major = ((& node --version) -replace '^v', '').Split('.')[0]
        [int]$major -ge 22
    }

    New-AosStep -Name 'git present' -Test { Test-AosCommand 'git' }

    New-AosStep -Name 'Python 3.11+ present (ML sidecars)' -Test {
        if (-not (Test-AosCommand 'python')) { return $false }
        $parts = ((& python --version 2>&1) -replace '[^0-9\.]', '').Split('.')
        if ($parts.Count -lt 2) { return $false }
        ([int]$parts[0] -gt 3) -or ([int]$parts[0] -eq 3 -and [int]$parts[1] -ge 11)
    }
)
