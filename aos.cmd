@echo off
REM AgenticOS agent launcher. Double-click this, or run `aos` from a terminal.
setlocal
set "AOS_ORCH=%~dp0src\orchestrator"

if not exist "%AOS_ORCH%\dist\agent.js" (
  echo Building the agent...
  pushd "%AOS_ORCH%"
  call npm.cmd install --no-fund --no-audit >nul 2>&1
  call node "node_modules\typescript\bin\tsc"
  popd
)

if not exist "%AOS_ORCH%\dist\agent.js" (
  echo Build failed. Run provisioning\Install-Aos.ps1 and try again.
  pause
  exit /b 1
)

node "%AOS_ORCH%\dist\agent.js" %*
endlocal
