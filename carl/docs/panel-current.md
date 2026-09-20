# Current command panel

Updated September 20, 2026. The panel is an app opened when wanted. Closing it leaves the
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

## Compatibility and limits

Old project records, protocol fields and provider APIs remain so stored data and older
clients can still be read. Issue 61 is complete for navigation and the task view, but total
removal of the backend project feature is still outstanding. No stored projects were deleted.

The panel does not invent work to make idle agents look busy. An idle conversational agent
and a separate ready repair can both be true. The service observation and regression tests
verify those states without opening a desktop window.
