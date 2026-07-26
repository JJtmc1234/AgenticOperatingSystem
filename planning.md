# planning

Idea broken into executable chunks. Effort assumes part time solo work. Dates are targets,
not commitments.

## principle that orders the phases

Build every capability as an idempotent provisioning module, install it on the live
machine, use it daily. The custom image consumes those modules at the end. This is why
phase 6 is cheap and why value arrives in phase 1.

## phases

| phase | chunk | effort | status |
|---|---|---|---|
| 0 | foundation, contracts, provisioning runner | 1 session | done |
| 1 | capability broker and four MCP servers | 1 to 2 weeks | in progress |
| 2 | TypeScript orchestrator, routines, memory | 1 to 2 weeks | not started |
| 3 | heads up display, hotkey, tray, approvals | 1 week | not started |
| 4 | sensors and proactivity as a Windows service | 1 to 2 weeks | not started |
| 5 | computer use vision fallback | 3 to 5 days | not started |
| 6 | the custom Windows image | 1 to 2 weeks | not started |
| 7 | kernel minifilter, optional | 2 to 3 weeks | not started |

## phase 0, foundation

Solution scaffold. `Aos.Core` holds the safety contracts. Provisioning runner establishes
the idempotent module pattern from the first commit.

Done when `dotnet build` succeeds and `Install-Aos.ps1 -WhatIf` lists planned actions
without changing anything.

## phase 1, capability servers and broker

Broker first, then the servers on top of it.

| server | scope |
|---|---|
| aos-windows | UIAutomation, window and process control, screen capture |
| aos-files | content indexed search, USN journal recent activity, safe organize |
| aos-shell | policy gated PowerShell, raw shell stays denied |
| aos-apps | Gmail and Google Calendar over OAuth2, browser control over CDP |

Done when a request like "find every PDF I touched this week and file them by project"
works from Claude Code, with the audit log showing each gated call. Also triaging tomorrow
calendar and drafting replies to unread mail.

## phase 2, orchestrator

TypeScript service on the Claude Agent SDK, consuming the phase 1 servers. Routines as
declarative definitions: morning brief, inbox triage, end of day shutdown, weekly review.
Scheduled through Windows Task Scheduler. Memory in SQLite, kept separate from transcript
history so routines have durable context. A local model handles high frequency
classification, Claude handles reasoning and writing, and a hard token budget guard caps
spend.

Done when the morning brief runs unattended and lands somewhere it gets read.

## phase 3, heads up display

C sharp WebView2 host rather than Electron. The WebView2 runtime already ships with
Windows 11, and a C sharp host gives native tray, always on top overlay, and global
hotkeys without a second Chromium. Global hotkey opens a command palette from anywhere.
Streaming responses, approval prompts for gated calls, kill switch, audit viewer.

Done when one hotkey takes typed or spoken intent to a visible result without leaving the
current application.

## phase 4, sensors and proactivity

`Aos.Service` as a real Windows service hosting ETW consumers, a USN journal watcher, WMI
subscriptions, and window focus tracking, feeding an event bus. Activity ledger drives the
weekly review and context reconstruction. Proactive nudges react to events rather than
prompts, rate limited and opt in per event class. Optional meeting capture runs audio
through local faster whisper on the 4060, free and offline.

Done when the service survives reboot and the weekly review is accurate.

## phase 5, vision fallback

Screenshot to Claude vision to coordinate actions through SendInput. Exposed as a distinct
higher cost tool the agent reaches for only when UIAutomation fails. The tool description
must state that ordering, because vision is slower, pricier, and more brittle.

Done when an application with no accessible control tree can be driven end to end.

## phase 6, the custom image

Install the Windows ADK with the WinPE add on. Set up VMware Workstation or VirtualBox as
the test target, since Hyper V is unavailable on Windows 11 Home. `Build-Image.ps1` mounts
install.wim, applies the provisioning modules offline, injects drivers and unattend.xml,
sets the service to auto start, unmounts, and builds an ISO with oscdimg. Secrets come from
a file supplied at build time and are never committed.

Done when a clean virtual machine boots from the ISO and the agent greets you at first
login with no manual setup.

## phase 7, kernel minifilter, optional

Only if phase 4 telemetry proves insufficient. Honest cost is WDK setup, a minifilter
driver project, test signing with Secure Boot disabled, and a real risk of bugchecks. VM
only, never the host, because test signing breaks BitLocker recovery flows and some anti
cheat.

Done when the driver loads in the VM, streams filesystem events, and survives a stress run
without a bugcheck.

## open items

1. Secrets storage. Windows Credential Manager over DPAPI is the pragmatic choice. Decide
   before the first API key is written anywhere.
2. Elevation split. The service needs administrator rights for ETW and system changes. The
   display and orchestrator should run as the normal user. Confirm rather than running
   everything elevated.

Mail and calendar is settled. Google Workspace, not Microsoft Graph.
