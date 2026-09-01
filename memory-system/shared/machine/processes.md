# What runs on Tensor

## The agent stack

Managed by systemd user units in `~/.config/systemd/user/`.

- `carl-slack` reads and answers Slack.
- `carl-listen` is the voice surface.
- `carl-aec` is audio echo cancellation.
- `carl-army` supervises the ten agents. Restarting it ends ten claude processes and starts ten
  replacements, which costs real money, so do not restart it for a rebuild that does not touch
  them.
- `miles.timer` fires `miles.service` every two hours.

The panel is not a service. It is two processes started by hand from
`~/Projects/carl-integration`: `./target/release/carl panel` is the backend on
`~/.carl/panel/panel.sock`, and `./target/release/carl-panel` is the window.
`restart-panel.sh` rebuilds and restarts the pair and also redeploys `~/.local/bin/carl`.

## Check for a second actor before believing an impossible bug

**This has caused three separate phantom bugs.** A stale background process was the cause every
time, and each time it looked like a logic error.

- A two day old `run.py` ate the RCON connection cap and lagged the game.
- Two coordinators ran on one socket.
- A 31 minute `science_loop.py` was driving agent characters, which read as the agents drifting.

Run `ps -eo pid,etime,args | grep python` and read the elapsed column before you debug
something that should be impossible.

A process having a parent is not the same as it being an orphan. Six `claude` processes named
Carl looked like a leak on 2026 08 29 and were four from `carl slack`, one from `carl listen`
and one from `carl supervise`. Check the parent before killing anything.
