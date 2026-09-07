# Iris issue workflow

Iris investigates committed source and publishes concise GitHub issues after independent review.
Carl delegates through Adrian to Iris. Runtime managed investigators are scoped to one source
batch and have no tools, shell, network tools, editing tools or publication authority. The
runtime alone fetches source and writes reviewed issues through argument arrays.

## Run

```sh
carl iris run --repo JJtmc1234/AgenticOperatingSystem --request "Inspect the permission workflow"
carl iris status
carl iris run --repo JJtmc1234/AgenticOperatingSystem --draft
```

Install with `bash integrations/iris/install.sh`, then enable the user timer with
`systemctl --user enable --now aos-iris.timer`. Installation preserves existing configuration.
New installations enable publication because JJ authorized it for this workflow.
No sudo or Python dependencies are needed. Git, GitHub CLI and authenticated Claude are required.
The existing Claude hooks and approval settings are unchanged. The tool-free Claude processes
retain user settings, including any trusted lifecycle hooks, but do not load repository settings.

## Triggers and scope

Manual requests start immediately. The timer polls default branch commits every five minutes.
Any new commit becomes eligible, so a feature does not need a particular commit title.
An hourly check discovers repositories and checks source revisions. Unchanged completed revisions
are skipped without model calls. Archived or empty repositories are reported as skipped.

By default, all repositories owned by JJtmc1234 are discovered. Narrow `repositories` in
`~/.carl/iris/config.json` to an explicit list if desired. Other owners are refused. Source is
fetched into private bare caches. Working tree changes are never mistaken for committed code.

Only supported source and configuration file types are scanned. Documents, lockfiles, generated
build trees, binaries, symlinks, files over 64 KiB and credential-bearing source are excluded.
Reports include excluded file counts. Source batches cover every eligible file rather than only
a fixed initial slice. Full coverage can require multiple runs and is reported explicitly.

## Investigation and publishing

Two independent investigators examine correctness and efficiency. Every finding must quote
actual committed source at a valid line and identify a mechanism, impact and proposed test.
A third investigator rejects unsupported findings and duplicates after reading relevant existing
issues and comments. Findings outside the assigned batch are rejected. No test result is invented.
Published reports explicitly label source analysis and state that runtime reproduction was not
performed. Performance claims need a source-supported mechanism, not invented timing figures.

Major findings get individual issues. Related minor findings are grouped by component and type.
Stable issue and finding markers cover both open and closed issues. A timed out publication is
reconciled before another run can create anything. Successful creation is read back and checked.
Iris never closes issues, edits existing issues, adds labels or sends email through this workflow.

## Limits and continuity

Defaults are one batch and at most three issues per run, at most $0.50 per model call,
$1.50 per run and $5 per UTC day. Unused or failed call reservations are conservatively retained
for that day. These are model budget limits, not guarantees about how a subscription bills.
A batch reserves room for two investigators and a reviewer before starting. Remaining source
batches and partially published reviewed plans stay queued. Set larger limits explicitly in the
configuration when faster coverage is worth the additional usage.

`events.jsonl` is the durable record. Budgets, completed batches and publication state are folded
from it. A lock prevents overlapping manual and scheduled runs. Corrupt journals fail closed.
Derived `latest-report.md` and `latest-report.json` distinguish publication, drafts, queued work
and failures. Repo and model failures never manufacture issues. Cached Git source is outside the
500 MB Chroma budget. This workflow does not change Chroma configuration.

## Validation

```sh
PYTHONPATH=integrations/iris python3 -m unittest discover -s integrations/iris/tests -v
cargo test --manifest-path carl/Cargo.toml --workspace
```

The Python tests use real temporary Git repositories and mocked model and GitHub writes.
They cover secret exclusion, committed snapshots, complete batching, literal command arguments,
budgets, concurrent runs, malformed results, source mismatch, duplicate prevention, draft to
publish conversion and recovery after publication limits. Live verification is recorded separately
so mocked issue creation is never presented as evidence of actual publication.
