# Nexus model policy

JJ requested stronger models for important agents on 2026 09 19.

| Role | Selected model |
| --- | --- |
| Carl, chief | `claude-fable-5-1` |
| Adrian, engineering lead | `claude-fable-5-1` |
| Serena, security lead | `claude-fable-5-1` |
| Evan, Iris, Mason, Nora, Olivia, Miles, Rowan | `claude-opus-5` |
| Evan's separate repair workflow | `claude-opus-5` |

Persistent selections live in `~/.carl/army/NAME/config.json`. The repair selection
lives in `~/.carl/evan/config.json`. The army was restarted while idle and all ten
process argument lists were checked. No global Claude default was changed.

A bounded tool-free Fable 5.1 check succeeded before this upgrade. Its usage report
listed only `claude-fable-5-1`, with a reported cost of $0.04560275. This confirms
that call used the selected model, not that future provider fallback cannot occur.
The earlier Fable 5 check included an Opus 4.8 fallback.
[Claude model configuration](https://code.claude.com/docs/en/model-config) documents
model selection and fallback behavior.

Evan keeps the existing limits of $0.50 per call, $1.50 per run and $5 per UTC day.
Those workflow limits do not govern the persistent conversational agents.


The lead upgrade exposed an existing idle-session restart defect. The supervisor
now records a conversation as established only after a completed answer. Starting
an idle Claude process alone does not create a transcript. This change preserves
used session IDs and avoids resuming newly launched sessions that received no work.
