# Panel protocol version 2

The backend and GUI speak JSON lines over `~/.carl/panel/panel.sock`.
Every request and reply carries `v: 2`. A version mismatch is refused with both versions
named. Install the backend and GUI together. A rejected frame does not close the socket.

Version 2 removes Projects. `PanelSnapshot` contains `seq`, `at`, `carl`, `agents`,
`tasks` and `diagnostics`. `TaskView` contains its id, goal, owner, assigner, parent,
status, attempts, verification requirements, review and timestamps. It has no project field.
Historical journal events with a project field remain readable through unknown-field handling.
No journal or historical project file is rewritten during this upgrade.

```json
{"v":2,"id":"status","ask":"snapshot"}
{"v":2,"id":"status","reply":"snapshot","snapshot":{"seq":0,"at":0,"carl":{"status":{"known":"unknown"},"pending":[],"objectives":[],"recent_delegations":[]},"agents":[],"tasks":[],"diagnostics":[]}}
```

Ping, subscriptions, permission questions, commands, telemetry, workflow updates and
workspace actions retain their existing shapes. They use `v: 2`. The [historical contract](panel-v1.md)
describes those shapes. Its Projects sections are retired, not optional features.

Tasks come from the append-only army journal. Telemetry and Evan workflow updates do not
advance its sequence. Reconnection replaces the snapshot before applying subsequent events.
Empty measurements are unknown, never zero. No live failure switches to demonstration data.
