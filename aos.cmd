@echo off
REM AgenticOS launcher.
REM
REM   aos                          interactive agent
REM   aos list                     show the routines
REM   aos daily-brief              run a routine
REM   aos tidy-downloads --commit  run a routine with its own flags
REM
REM Dispatching on the first argument, because the daily brief tells you to run
REM `aos tidy-downloads` and without this that string would be typed at the agent as a
REM prompt. A documented command that quietly does something else is worse than no command.
setlocal
set "AOS_ORCH=%~dp0src\orchestrator"

if not exist "%AOS_ORCH%\dist\agent.js" (
  echo Building...
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

REM No argument means the interactive agent, which is what the hotkey and a double-click do.
if "%~1"=="" (
  node "%AOS_ORCH%\dist\agent.js"
  endlocal
  exit /b %ERRORLEVEL%
)

REM Anything else is a routine name. index.js rejects an unknown one and lists the real ones,
REM so a typo gets a useful answer rather than being silently sent to the model.
node "%AOS_ORCH%\dist\index.js" %*
endlocal
exit /b %ERRORLEVEL%
