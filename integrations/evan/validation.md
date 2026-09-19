# Omarchy validation

On 2026 09 19, all 32 Evan tests passed on Nexus with `EVAN_SANDBOX_TESTS=1`.
This executed the failing regression and repaired test in Bubblewrap, created a local
Git commit, checked network isolation and exercised timeout cleanup. Model responses
and GitHub calls were fixtures. No live model repair or GitHub PR publication was tested.

The shared Iris transport passed its 92 tests. Carl and the panel passed workspace
formatting, Clippy with warnings denied and the Rust tests. Parallel tests twice hit
transient executable file busy errors in different subprocess fixtures. The sequential
suite passed without changing those fixtures.

The installed Evan runtime has no repository policies and publication is disabled.
No automatic repair timer was enabled. Real repairs require a configured repository,
explicit source and test paths, a passing isolated baseline and a confirmed Iris issue.
Deliberate Iris validation reports remain excluded from the production queue.

The input integrity regressions failed before the fix and passed afterward in the real
sandbox. Edited, deleted and linked inputs are rejected. Generated output is allowed.
The check runs between commands, so a later command cannot hide an earlier mutation.
