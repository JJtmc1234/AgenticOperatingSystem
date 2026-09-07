# Iris manual flow verification

Verified on Tensor on 7 September 2026. No GitHub test issues were created.

## Real execution

The normal workflow fetched AOS from GitHub, loaded open and closed issues, and delegated one
committed file to two tool-free model investigators and an independent reviewer. The file was
`carl/src/pushback.rs` at `20297ec61ae0717201d485d1fc0ce0820dc01221`. The mode was draft.
The review reserved $0.48 inside the existing $5 daily limit, without raising that limit.

Three local drafts resulted. Operator verification compiled the exact committed detection module
in an isolated Rust executable. The actual results were:

| Input | Result |
|---|---|
| `missed("no thats exactly right, thanks", "Go for steel.")` | `Some(Corrected)` |
| `missed("there is not that much coal on this patch", "Go for steel.")` | `Some(Corrected)` |
| `repeated("iron iron iron please", "iron copper coal stone")` | `Some(Repeated)` |

The first two inputs demonstrate false correction signals. In the third, repeated words give
3/4 overlap. Distinct word sets give 1/4. This is a synthetic example of the mechanism.
No live chat response or deployed application behavior was tested.

The original repetition draft used an example whose unique words all overlapped, so that example
did not demonstrate a difference. The operator corrected the saved draft and reviewed plan,
recorded the correction in the journal, and removed untested downstream claims. The model reviewer
prompt now explicitly requires tracing examples and distinguishing observations from assumptions.
Those revised prompt instructions have focused mock coverage but have not had another live model
run because the daily allowance is nearly exhausted.

## Focused automated checks

66 Python tests pass. Models and GitHub writes are mocked in these tests. Temporary Git repositories
are real. Coverage includes specific requests, enhancements labelled as requests, exact source
selection, scope rejection, missing source, no findings, duplicate links, stale draft publication,
closed issues, grouping, budget stops, concurrency and durable manual reports.

Five regressions for budget diagnostics, startup failure status, durable reports, grouping and
suggested directions failed against the previous code. Four publication regressions also failed
against the previous publication function. No Rust application files changed in this task.

## Boundaries

This verifies one source file, not a full AOS audit. A source excerpt and model review are not a
universal proof of a defect. Runtime reproduction remains explicitly labelled when absent.
The timer detects all new default branch commits rather than semantically identifying new features.
Existing title and marker checks plus bounded model review reduce duplicates but cannot guarantee
that differently worded historical issues will always be recognized. Publishing was verified
through mocks in this task, with the existing real issue 45 left untouched.
