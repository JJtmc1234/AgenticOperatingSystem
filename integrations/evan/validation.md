# Omarchy validation

On 2026 09 19, all 32 Evan tests passed on Nexus with `EVAN_SANDBOX_TESTS=1`.
This exercised repair preparation, local commits, test isolation and timeout cleanup.
Input integrity regressions failed before the fix and passed afterward. Edited, deleted
and linked inputs are rejected after every command. Generated test output is allowed.

The shared Iris transport passed 92 tests. Carl and the panel passed formatting,
Clippy with warnings denied and 1416 Rust tests, with one ignored. Parallel tests had
transient executable file busy errors. The sequential suite passed unchanged.

## First real repair

Evan ran against confirmed Iris issue 2 in `JJtmc1234/Holoprojector`, using Opus 5
for regression authoring, repair and independent review. Six fixed keyboard baseline
checks passed. The new regression failed against the original source. Seven new tests
and the six baseline checks passed after repair. Independent review accepted the fix.

Prepared commit: `fbcbe9a030ba6bcf526112860f3c462c58343bdd`.
Branch: `evan/issue-2-68ed950543636ae0a1dcfe11`.
Evidence on Nexus:
`~/.carl/evan/attempts/68ed950543636ae0a1dcfe11-2/evidence.json`.

The repair binds pause to `i`, next to resume on `o`, while preserving pointer switching
on `p`. Only the allowed input mapping and new regression test changed. This exercised
real model responses and real isolated tests. It did not run the full Holoprojector
pytest suite, launch its display or publish a GitHub PR.

## Current operation

The five minute timer is enabled with the exact policy in
[policies/holoprojector.json](policies/holoprojector.json). Publication remains disabled.
Prepared commits stay local. Deliberate Iris validation reports remain excluded.
The existing limits remain $0.50 per call, $1.50 per run and $5 per UTC day.
[Model policy](../../carl/etc/model-policy.md) records the separate persistent agent tiers.
