# Builds the orchestrator and schedules its routines.
#
# Task Scheduler rather than a resident service: a routine that runs once a day does not
# justify a process sitting in memory all day, and Scheduler already handles missed runs
# after the machine was asleep, which a naive timer loop does not.

$orchestrator = Join-Path $RepoRoot 'src\orchestrator'
$entryPoint   = Join-Path $orchestrator 'dist\index.js'
$taskName     = 'AgenticOS daily brief'
$briefTime    = '07:30'

@(
    New-AosStep -Name 'orchestrator dependencies installed' -Test {
        # Compares npm's own install marker against the manifests rather than probing for one
        # package. Testing for typescript alone answered "is typescript there", not "is the
        # tree in sync": adding a dependency to package.json left the Test passing and the
        # build then failed on a missing module.
        $marker = Join-Path $orchestrator 'node_modules\.package-lock.json'
        if (-not (Test-Path -LiteralPath $marker)) { return $false }

        $installedAt = (Get-Item -LiteralPath $marker).LastWriteTimeUtc

        foreach ($manifest in @('package.json', 'package-lock.json')) {
            $path = Join-Path $orchestrator $manifest
            if (-not (Test-Path -LiteralPath $path)) { continue }
            if ((Get-Item -LiteralPath $path).LastWriteTimeUtc -gt $installedAt) { return $false }
        }

        $true
    }.GetNewClosure() -Set {
        # npm.cmd, not npm. The PowerShell shim mangles the first argument in this
        # environment, turning "install" into "pm".
        Push-Location $orchestrator
        try {
            $output = & npm.cmd install --no-fund --no-audit
            if ($LASTEXITCODE -ne 0) { throw "npm install failed:`n$($output -join "`n")" }
        }
        finally { Pop-Location }
    }.GetNewClosure()

    New-AosStep -Name 'orchestrator built' -Test {
        if (-not (Test-Path -LiteralPath $entryPoint)) { return $false }

        $newestSource = Get-ChildItem -Path (Join-Path $orchestrator 'src') -Recurse -File -Filter '*.ts' -ErrorAction SilentlyContinue |
            Sort-Object LastWriteTimeUtc -Descending | Select-Object -First 1

        if (-not $newestSource) { return $true }
        (Get-Item -LiteralPath $entryPoint).LastWriteTimeUtc -ge $newestSource.LastWriteTimeUtc
    }.GetNewClosure() -Set {
        Push-Location $orchestrator
        try {
            $output = & node 'node_modules\typescript\bin\tsc'
            if ($LASTEXITCODE -ne 0) { throw "tsc failed:`n$($output -join "`n")" }
        }
        finally { Pop-Location }
    }.GetNewClosure()

    New-AosStep -Name "scheduled task: $taskName (daily $briefTime)" -Test {
        $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        if (-not $task) { return $false }

        # Everything the step's name promises gets checked, not just existence. A task that is
        # disabled, or that fires at a time nobody chose, is a brief that never arrives, and
        # the old Test reported ok for both. Re-registering is cheap, so any drift re-runs Set.
        if ($task.State -eq 'Disabled') { return $false }

        # Confirm it still points at this checkout, not a stale path.
        $matchesEntry = @($task.Actions | Where-Object { $_.Arguments -like "*$entryPoint*" }).Count -gt 0
        if (-not $matchesEntry) { return $false }

        $daily = @($task.Triggers | Where-Object {
            $_.CimClass.CimClassName -eq 'MSFT_TaskDailyTrigger' -and
            $_.Enabled -and
            $_.StartBoundary -and
            ([datetime]$_.StartBoundary).ToString('HH:mm') -eq $briefTime
        })

        $daily.Count -gt 0
    }.GetNewClosure() -Set {
        $node = (Get-Command node).Source

        $action = New-ScheduledTaskAction -Execute $node -Argument "`"$entryPoint`" daily-brief" `
            -WorkingDirectory $orchestrator
        $trigger = New-ScheduledTaskTrigger -Daily -At $briefTime
        # StartWhenAvailable catches up after the laptop was closed at 07:30, which is the
        # normal case rather than the exception.
        $settings = New-ScheduledTaskSettingsSet -StartWhenAvailable `
            -DontStopIfGoingOnBatteries -AllowStartIfOnBatteries `
            -ExecutionTimeLimit (New-TimeSpan -Minutes 10)

        Register-ScheduledTask -TaskName $taskName -Action $action -Trigger $trigger `
            -Settings $settings -Description 'Repo state, open issues and Downloads clutter.' `
            -Force | Out-Null
    }.GetNewClosure()
)
