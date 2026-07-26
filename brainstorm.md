# brainstorm

How this project idea was reached.

## the want

Make Windows agent native. Agents should be part of the operating system, not an app you
alt tab into. Goal is personal productivity, not a product for other users.

## four readings of "agentic operating system built off Windows"

| reading | verdict |
|---|---|
| agent layer on top of Windows | fastest, but not really an OS |
| replace explorer.exe as the shell | high risk, high wow, rebuilds the desktop from zero |
| custom Windows image with the agent baked in | chosen, real OS distro feel |
| ordinary desktop app | rejected, no system integration |

Chosen: custom Windows image. Agents act through native APIs, MCP servers, computer use
vision, and kernel hooks. Stack is hybrid, C sharp for deep Windows access and TypeScript
for the agent loop.

## the tension that shaped everything

A custom image plus kernel drivers is the slowest iteration loop on Windows. Edit, rebuild
the WIM, boot a VM, test. Twenty to forty minutes per cycle. Driver work needs an EV
certificate with Microsoft attestation signing, or test signing mode with Secure Boot off.

If the image were the development environment, months would go to build plumbing before
any productivity gain arrived.

## the inversion

The custom image is a packaging target, not a development environment.

Every capability is an idempotent provisioning module that installs onto a live machine.
Daily dogfooding starts in phase 1. The image build applies those same tested modules
offline to a mounted WIM, which is nearly free once provisioning is proven.

Same logic for observability. ETW, the NTFS USN journal, and WMI give most of what a
kernel minifilter gives, with none of the signing cost. The real driver waits until last
and stays in a VM.

## consequence we liked

Phase 1 delivers value through Claude Code and Claude Desktop, both already installed,
before any custom user interface exists.

## constraints found by probing the machine

Windows 11 Home has no Hyper V, so image and driver testing needs VMware Workstation or
VirtualBox. The Windows ADK is not installed, which phase 6 requires. Hardware is
comfortable at 20 cores, 64 GB RAM, and an RTX 4060, so local Whisper and a small local
model can absorb high frequency work and keep token cost near zero.

See [planning.md](planning.md) for the phase breakdown.
