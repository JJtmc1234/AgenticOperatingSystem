# Current command panel

Updated September 22, 2026. The panel is an app opened when wanted. Closing it leaves the
background army, voice, Slack and repair timer running. On JJ's Omarchy machine the
`carl-panel.service` unit is disabled. `carl-panel-backend.service` serves the local socket.
The backend does not require an open window.

## Opening and layout

Run `carl-panel` or use its application launcher entry. It opens as a resizable window.
Fullscreen requires `--fullscreen`. Demonstration data requires `--mock`. A failed live
connection remains a failed connection and does not silently switch to invented activity.
`--toggle` can launch a panel if one is absent, so it is an intentional user action.

The navigation is Overview, Carl, Agents, Diagnostics and To-do. Narrow windows use compact
navigation and fewer columns. The short Carl view keeps its message composer reachable.
The To-do view replaces the old Projects navigation and shows task goals, owners and status.
Finished work is collapsed. A scheduled repair such as Evan's appears separately from a
conversational assignment, because they are different sources of work.

## Why these diagnostics are visible

This is the rationale requested by AOS issue 59, recorded here for review.

| Visible information | Why it helps |
| --- | --- |
| Component name | Identifies which service or subsystem needs investigation |
| Fault or blocker state | Indicates something that can prevent useful work |
| Short summary | Explains the reported problem without requiring raw measurements |
| Stale reading label | Prevents an old healthy reading from implying current health |
| Investigate action | Opens the relevant evidence rather than requiring a search |
| Collapsed unmeasured list | Makes missing evidence available without calling it a failure |
| Optional full measurements | Supports debugging while keeping the default view short |

Healthy, current measurements are hidden by default. Unknown readings are separate from
fault counts. No readings produces an explicit unknown state. Missing measurements are
never converted to zero. The opt-in full view retains the detailed component inventory.

## Task and repair truth

Only the journal supplies delegated tasks. Accepted or abandoned work cannot become an
agent's current assignment through a stale reference. JJ has a human authority card,
because he is not a supervised agent. A missing agent reading is labeled unavailable.

Evan's repair report can show a prepared repair alongside a blocked issue. Unknown report
vocabulary is unavailable rather than idle. Ready repairs do not clear blockers. The
review command validates repository and issue arguments before displaying them.

## Retired Projects and protocol upgrade

The Projects store, provider API, milestone events, UI and task links are removed.
The To-do list uses recorded assignments directly, including owners, requirements,
blocked work, finished work and Evan repairs. Old journal lines containing `project`
still load. Historical files under `~/.carl/projects` are left untouched and are not read.

The panel protocol is version 2 because snapshots no longer contain `projects`.
Install the backend and GUI together. Old protocol clients receive an explicit version
mismatch instead of a partial snapshot. See [the current wire contract](panel-v2.md).

The panel does not invent work to make idle agents look busy. An idle conversational agent
and a separate ready repair can both be true. Tests cover both without opening a window.

## Submitting Evan's reviewed work

Run `carl evan review --repo OWNER/REPO --issue NUMBER` to inspect the exact prepared
commit and evidence. After reviewing it, `carl evan submit --repo OWNER/REPO --issue NUMBER`
publishes that one repair as a draft PR. It rechecks the issue, source, repository policy
and clean prepared checkout. It never merges and does not enable automatic publication.
Changed inputs require revalidation. Repeated submissions reuse the recorded PR.
