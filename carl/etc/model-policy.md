# Nexus model policy

JJ requested stronger models for important agents on 2026 09 19.

| Role | Selected model |
| --- | --- |
| Carl, chief | `claude-fable-5` |
| Adrian, engineering lead | `claude-fable-5` |
| Serena, security lead | `claude-fable-5` |
| Evan, Iris, Mason, Nora, Olivia, Miles, Rowan | `claude-opus-5` |
| Evan's separate repair workflow | `claude-opus-5` |

Persistent selections live in `~/.carl/army/NAME/config.json`. The repair selection
lives in `~/.carl/evan/config.json`. The army was restarted while idle and all ten
process argument lists were checked. No global Claude default was changed.

A bounded tool-free Fable check returned OK. Its usage report included both Fable 5
and Opus 4.8. This confirms availability, not that provider fallback never occurs.
[Claude model configuration](https://code.claude.com/docs/en/model-config) documents
model selection and fallback behavior.

Evan keeps the existing limits of $0.50 per call, $1.50 per run and $5 per UTC day.
Those workflow limits do not govern the persistent conversational agents.
