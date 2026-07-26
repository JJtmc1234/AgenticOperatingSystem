#Requires -Version 5.1
<#
.SYNOPSIS
    Applies AgenticOS provisioning modules to this machine.

.DESCRIPTION
    This script is the definition of "the OS". Every module is a list of idempotent
    Test/Set steps: Test reports whether the desired state already holds, Set makes it
    hold. Running twice must report zero changes on the second pass -- Phase 6 builds the
    custom image by applying these same modules offline to a mounted WIM, so idempotence
    here is what keeps image builds cheap.

.PARAMETER Module
    Apply only modules whose filename matches this wildcard. Default: all.

.PARAMETER WhatIf
    Report what would change without changing anything.

.EXAMPLE
    .\Install-Aos.ps1 -WhatIf
.EXAMPLE
    .\Install-Aos.ps1 -Module '10-*'
#>
[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [string] $Module = '*',
    [string] $RepoRoot = (Split-Path -Parent $PSScriptRoot)
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# --- Step contract -----------------------------------------------------------------
# Test: scriptblock returning $true when the desired state already holds.
# Set:  scriptblock that establishes it. Never called when Test is already $true.
#
# MODULE AUTHORING RULE
# Steps are collected from a module, then invoked after that module's script scope is gone.
# A scriptblock does not keep its defining scope alive, so any module-local variable a step
# references MUST be captured with .GetNewClosure() at build time:
#
#     $p = $path
#     New-AosStep -Name '...' -Test { Test-Path $p }.GetNewClosure()
#
# The catch: GetNewClosure snapshots the module scope, and that closed-over scope chain does
# not reach this runner's script scope. So a helper *function* called from inside a closure
# would not resolve either. Helpers are therefore defined in the global scope, which is
# visible from every scope including closures. They are removed again on exit.
$script:AosHelpers = @(
    'New-AosStep', 'Get-AosPath', 'Test-AosCommand', 'Test-AosProperty', 'Test-AosAdmin',
    # Defined by modules rather than here, but still global and still worth cleaning up.
    'Get-AosNewestSourceUtc')

function global:New-AosStep {
    param(
        [Parameter(Mandatory)][string]        $Name,
        [Parameter(Mandatory)][scriptblock]   $Test,
        [scriptblock]                         $Set,
        [switch]                              $RequiresAdmin
    )
    [pscustomobject]@{
        Name          = $Name
        Test          = $Test
        Set           = $Set
        RequiresAdmin = [bool]$RequiresAdmin
    }
}

function global:Get-AosPath {
    param([Parameter(Mandatory)][string] $Path)
    [Environment]::ExpandEnvironmentVariables($Path)
}

function global:Test-AosCommand {
    param([Parameter(Mandatory)][string] $Name)
    [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

# Null-safe property probe. $obj.PSObject.Properties.Name returns $null for an object with
# no properties, so calling .Contains() on it throws.
function global:Test-AosProperty {
    param(
        [Parameter(Mandatory)] $InputObject,
        [Parameter(Mandatory)][string] $Name
    )
    if ($null -eq $InputObject) { return $false }
    $null -ne $InputObject.PSObject.Properties[$Name]
}

function global:Test-AosAdmin {
    $id = [Security.Principal.WindowsIdentity]::GetCurrent()
    (New-Object Security.Principal.WindowsPrincipal $id).IsInRole(
        [Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Modules read these unqualified ($AosRoot, $RepoRoot). Do not use $script: in a module
# file -- that resolves to the module's own scope, not this one. Unqualified lookup walks
# up the scope chain and finds these.
$AosRoot = Get-AosPath '%LOCALAPPDATA%\AgenticOS'
$isAdmin = Test-AosAdmin

# --- Runner ------------------------------------------------------------------------
$moduleFiles = Get-ChildItem -Path (Join-Path $PSScriptRoot 'modules') -Filter '*.ps1' -File |
    Where-Object { $_.Name -like $Module } |
    Sort-Object Name

if (-not $moduleFiles) {
    Write-Warning "No provisioning modules matched '$Module'."
    exit 0
}

$tally = [ordered]@{ Ok = 0; Changed = 0; Would = 0; Skipped = 0; Failed = 0 }

foreach ($file in $moduleFiles) {
    Write-Host ""
    Write-Host "== $($file.BaseName)" -ForegroundColor Cyan

    $steps = & $file.FullName
    if (-not $steps) { continue }

    foreach ($step in @($steps)) {
        if ($step.RequiresAdmin -and -not $isAdmin) {
            Write-Host "   SKIP  $($step.Name) (needs elevation)" -ForegroundColor DarkYellow
            $tally.Skipped++
            continue
        }

        try {
            $satisfied = [bool](& $step.Test)
        }
        catch {
            Write-Host "   FAIL  $($step.Name) -- Test threw: $($_.Exception.Message)" -ForegroundColor Red
            $tally.Failed++
            continue
        }

        if ($satisfied) {
            Write-Host "   OK    $($step.Name)" -ForegroundColor DarkGray
            $tally.Ok++
            continue
        }

        if (-not $step.Set) {
            Write-Host "   FAIL  $($step.Name) -- unsatisfied and no remediation" -ForegroundColor Red
            $tally.Failed++
            continue
        }

        if (-not $PSCmdlet.ShouldProcess($step.Name, 'apply')) {
            Write-Host "   WOULD $($step.Name)" -ForegroundColor Yellow
            $tally.Would++
            continue
        }

        try {
            & $step.Set | Out-Null
        }
        catch {
            Write-Host "   FAIL  $($step.Name) -- $($_.Exception.Message)" -ForegroundColor Red
            $tally.Failed++
            continue
        }

        # Converge check: a Set that does not satisfy its own Test is not idempotent.
        if ([bool](& $step.Test)) {
            Write-Host "   SET   $($step.Name)" -ForegroundColor Green
            $tally.Changed++
        }
        else {
            Write-Host "   FAIL  $($step.Name) -- Set ran but Test still false" -ForegroundColor Red
            $tally.Failed++
        }
    }
}

Write-Host ""
Write-Host ("-- ok {0}  changed {1}  would {2}  skipped {3}  failed {4}" -f `
    $tally.Ok, $tally.Changed, $tally.Would, $tally.Skipped, $tally.Failed)

if (-not $isAdmin -and $tally.Skipped -gt 0) {
    Write-Host "   Re-run elevated to apply the skipped steps." -ForegroundColor DarkYellow
}

# Do not leave the global helpers behind in an interactive session. The Function: drive is
# session-wide, so "Function:\Name" is the path that actually removes a global function --
# "function:global:Name" is not a valid provider path and fails silently.
foreach ($helper in $script:AosHelpers) {
    if (Test-Path -LiteralPath "Function:\$helper") {
        Remove-Item -LiteralPath "Function:\$helper" -Force
    }
}

if ($tally.Failed -gt 0) { exit 1 }
exit 0
