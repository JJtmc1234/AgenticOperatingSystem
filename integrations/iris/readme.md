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

For a specific bug or requested enhancement, use the same reviewed publication path:

```sh
carl iris issue --repo JJtmc1234/AgenticOperatingSystem \
  --path carl/portal/page.js \
  --request "Check whether a slow chat refresh can display one message twice" --draft
```

`--path` selects an exact committed file. Repeat it for callers and tests. Missing, uncommitted,
excluded or sensitive files fail visibly. Without paths, review covers eligible source in bounded
batches and may need several runs. A specific issue request restricts the investigators to JJ's
requested problem. Enhancement requests are labelled as requests, not invented defects.

Keep `--draft` to inspect the result locally. Omit it to use the existing `publish` configuration.
No command line flag can enable publication when that configuration disables it. Run the same
command again to resume or publish an already reviewed draft without repeating completed model work.
A budget stop reports the reserved amount, required allowance and UTC reset time. It is not a
no-findings result. `carl iris status` links recent drafts and the latest manual report, which
scheduled polls do not overwrite.

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
actual committed source and identify a mechanism, impact and proposed test. If its line number
is wrong, an exact unique quotation can be located in the committed file. Invented quotations
and ambiguous locations are rejected.
Both investigators receive relevant open and closed issue bodies and fix comments before
reviewing source. A third investigator rejects unsupported findings and duplicates after reading
additional relevant history when needed. Findings outside the assigned batch are rejected. No test result is invented.
Published reports explicitly label source analysis and state that runtime reproduction was not
performed. Performance claims need a source-supported mechanism, not invented timing figures.

The independent reviewer can lower overstated severity without discarding a real defect.
Unsupported mechanisms, consequences and reproductions are still rejected.
Major findings get individual issues. Related minor findings are grouped by source file and type so unrelated components stay separate.
Stable issue and finding markers cover both open and closed issues. Publication refreshes issue
history even for stored drafts and capped runs. Existing matches return their URLs. A partial
individual-marker match conservatively suppresses the overlapping grouped plan. Differently worded
duplicates still depend on the bounded independent review, which reads up to five related issue bodies
and their latest ten comments. This is duplicate resistance, not a guarantee of semantic identity. An uncertain publication leaves a durable intent. Later runs first reconcile its exact marker
against GitHub. If it is still absent, the repository reports a failure and creates nothing,
including with `--force`. Absence from a list is not proof that a timed out write failed.
Successful creation is read back and checked.
Iris never closes issues, edits existing issues, adds labels or sends email through this workflow.

## Limits and continuity

Defaults are one batch of at most eight files and 64 KiB, with medium reasoning effort,
and at most three issues per run, at most $0.50 per model call,
$1.50 per run and $5 per UTC day. Unused or failed call reservations are conservatively retained
for that day. These are model budget limits, not guarantees about how a subscription bills.
A batch reserves room for two investigators and a reviewer before starting. Remaining source
batches and partially published reviewed plans stay queued. Set larger limits explicitly in the
configuration when faster coverage is worth the additional usage.

`events.jsonl` is the durable record. Budgets, completed batches and publication state are folded
from it. A kernel lock prevents overlapping manual and scheduled runs and releases automatically
when a process exits. A torn final append after a valid journal prefix is preserved separately
and repaired under that lock. Complete corrupt records fail closed. Hourly cursors are scoped
by request and publication mode, so a focused draft cannot delay a general scheduled review.
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

## Playwright browser tests

Install the separate browser dependencies with `bash integrations/iris/install-browser-tests.sh`,
then run `carl iris test`. Node 22.13 or newer with SQLite support is required. The installer
uses a pinned Playwright lockfile and downloads Chromium. It installs a reviewed snapshot of
the portal page and handlers with source hashes. Reinstall it after intended source changes.

The suite drives Chromium through real login, messaging and membership flows over loopback HTTP.
It uses the actual portal handlers and SQLite statements through a local D1 adapter. This is
not a Cloudflare runtime or deployment test. Synthetic accounts and an in-memory database keep
all test messages away from the live room. The native egui panel is outside Playwright scope.

Iris has no arbitrary command, URL or repository option through this test entry point.
The runner rejects modified source, records an audit event before execution, refuses an occupied
test server port, bounds execution time and reports a nonzero status for failures or empty runs.
Reports, source hashes, screenshots and failure traces live under `~/.carl/iris/browser-tests/`.
There is no model charge for running the browser suite. Investigation and issue review retain
their normal model budget. Browser test runs do not publish automatically.

The first corrected live suite ran through Adrian to Iris: five passed and one failed.
The failure reproduces duplicate rendered messages under overlapping poll responses.
On 9 September the application fix passed all six browser tests. The unchanged regression
failed first against the original page with one stored message and two displayed messages.
The display loop now ignores message IDs it has already rendered. See `homework-audit.md`.

The runner follows Playwright's [local web server configuration](https://playwright.dev/docs/test-webserver)
and [retrying assertions](https://playwright.dev/docs/test-assertions).

## Completion notifications

Run `python3 integrations/iris/install-notifications.py` to install `aos-notify` and configure
the user-level Codex completion hook. The installer preserves the previous config in a backup
and refuses to overwrite a different notification handler. New Codex sessions load the hook.
It follows the official [Codex notification configuration](https://learn.chatgpt.com/docs/config-file/config-advanced#notifications).

The installed Iris workflow sends local desktop notices after manual investigations, meaningful
scheduled investigations and browser tests. Idle polls stay quiet. Repeated identical idle
failures are suppressed. A missing desktop notification service does not fail the work.
Notification requests and delivery results are audited. Codex delivery results are recorded
in `~/.local/state/aos-notifications/events.jsonl`. Prompt and answer text are never displayed
or logged by the notifier. Linux needs `notify-send` and a desktop notification service.

Iris's first reviewed browser finding is published as
[issue 45](https://github.com/JJtmc1234/AgenticOperatingSystem/issues/45).

## Persistent results and notification location

`carl iris status` now shows recent published issues, the latest browser result and current
investigation state. Routine polls no longer replace that history with a waiting message.
The derived overview is `~/.carl/iris/overview.md`. On JJ's layout the installer links it from
`~/Projects/AOS/iris-status.md` so it is easy to find.

Notifications appear on the machine running the notifier. Ubuntu desktop acceptance does not
mean an alert reached another Arch machine. Run the notification installer in the Arch desktop
session too. An ID means `accepted_by_service`, while `visible_to_user` remains unknown.
Notices request persistence and register under AOS Notifications. Desktop environments may still
apply their own banner and do-not-disturb settings. No global desktop preference is changed.

## Current verification scope

The September 7 manual review used real GitHub source and real tool-free model investigators for
one file, `carl/src/pushback.rs`, at revision `20297ec61ae0`. It produced three local drafts and
published nothing. Operator inspection exercised the detection functions in an isolated Rust
executable and corrected one proposed example that did not demonstrate its claimed difference.
See `manual-verification.md` for evidence and remaining limits.

The existing timer is enabled on Tensor. It checks revisions every five minutes, resumes eligible
unfinished work hourly, and detects every new default branch commit. It does not classify whether
a commit is a completely new feature, and unchanged completed source is not repeatedly sent to
models. The existing owner-wide scope is preserved. Set an explicit `repositories` list to narrow
future reviews, and keep `--repo` on a manual request intended for one project.

## Through Carl in the graphical panel

Use the existing Carl conversation. For example, say:

> Have Iris review JJtmc1234/AgenticOperatingSystem for bugs and return drafts here.

Or:

> Ask Iris for her current status and the links to her latest issues.

Carl hands the request to Adrian, who delegates to Iris. Iris runs the controlled workflow and
returns the result through Adrian to Carl. Repository selection, file paths and draft intent
must survive both handoffs. The same chat receives real issue links, draft paths or the exact
reason work is queued. Status requests do not start a new review. There is no separate Iris
window, and JJ does not need a terminal for these requests.

## Resuming existing findings

Repeated requests retain existing open or closed issue links through the durable journal,
including a completed general review. This needs no additional investigator call. Links
found before this journal field existed may require a fresh review to recover.
The overview separates complete, queued, waiting and failed reviews. A waiting poll did
not perform a source review and is never counted as one.

## Fair progress during active development

New commits still invalidate previous source reviews. They no longer reset every scan to
the first alphabetical batch. Saved publication work is resumed first, then batches are
ordered by when their files were last reviewed for the same request. Files waiting for
their first review therefore get a turn even when commits arrive between runs.
No spending limits change. Historical entries without a request scope remain readable,
but start contributing to fair ordering after their next completed review.

## Smaller review allowance

Append `--call-budget 0.25` to `carl iris run` or `carl iris issue` to lower that run's
per-call model allowance. It cannot exceed the configured cap and does not change saved
settings or the daily limit. A cap too low for a model response fails visibly.
A complete review reserves enough allowance for both investigators and independent review.
No findings means no third model call and no issue publication.

The latest publication list shows at most ten links and states the total recorded count.
An older issue missing from that list is not evidence that it was closed or never published.

Current live review and Carl handoff evidence is in [final verification](final-verification.md).

The fifteen project live detection and repair exercise is recorded in
[September 17 verification](verification-20260917.md), including real issue links and limitations.
