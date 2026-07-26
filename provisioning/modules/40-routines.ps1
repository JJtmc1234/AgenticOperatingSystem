# Builds the orchestrator and schedules its routines.
#
# Task Scheduler rather than a resident service: a routine that runs once a day does not
# justify a process sitting in memory all day, and Scheduler already handles missed runs
# after the machine was asleep, which a naive timer loop does not.

$orchestrator = Join-Path $RepoRoot 'src\orchestrator'
$entryPoint   = Join-Path $orchestrator 'dist\index.js'
$taskName     = 'AgenticOS daily brief'

@(
    New-AosStep -Name 'orchestrator dependencies installed' -Test {
        Test-Path -LiteralPath (Join-Path $orchestrator 'node_modules\typescript\bin\tsc')
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

    New-AosStep -Name "scheduled task: $taskName (daily 07:30)" -Test {
        $task = Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        if (-not $task) { return $false }
        # Confirm it still points at this checkout, not a stale path.
        ($task.Actions | ForEach-Object { $_.Arguments }) -match [regex]::Escape($entryPoint)
    }.GetNewClosure() -Set {
        $node = (Get-Command node).Source

        $action = New-ScheduledTaskAction -Execute $node -Argument "`"$entryPoint`" daily-brief" `
            -WorkingDirectory $orchestrator
        $trigger = New-ScheduledTaskTrigger -Daily -At '07:30'
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
