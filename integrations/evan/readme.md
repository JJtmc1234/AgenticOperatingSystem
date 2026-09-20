# Evan issue repair

Evan is the issue fixer under Adrian and Carl. His runtime reads confirmed Iris issues,
prepares minimal tested fixes, and can submit draft PRs. It never merges or closes issues.

## Starting Evan

Install with `bash integrations/evan/install.sh` and rebuild Carl to expose `carl evan`.
The installer preserves any existing configuration. A fresh installation has no repositories
and publication disabled. It does not spend model allowance or enable the timer by itself.

Configure `~/.carl/evan/config.json` with explicit source paths, regression paths and fixed test
argument arrays. These are operator permissions, not model suggestions. Each repository must
belong to the configured owner. For example, a small Python project can use:

```json
{
  "owner": "JJtmc1234",
  "publish": false,
  "repositories": {
    "JJtmc1234/example": {
      "paths": ["src/example.py"],
      "test_paths": ["tests/test_example.py"],
      "commands": [["python3", "-m", "unittest", "discover", "-s", "tests", "-v"]]
    }
  }
}
```

Do not copy the example repository name as an actual assignment. Choose the real affected
files and a test command that already passes in the isolated environment. Add related source
paths if the issue needs more context. Missing dependencies or insufficient scope are blockers.

```sh
carl evan doctor
carl evan run --repo JJtmc1234/example --draft
carl evan status
carl evan review --repo JJtmc1234/Holoprojector --issue 2
systemctl --user enable --now aos-evan.timer
```

Set `publish` to true only when draft PR publication is authorized for these repositories.
A later run resumes an unchanged prepared fix without repeating model work. Each poll
checks that the prepared checkout is still clean and on its verified commit. A local
edit blocks the repair instead of leaving a stale ready state. `--draft` always
disables writes. `--retry` permits a new attempt after a blocked or interrupted local repair,
but cannot bypass uncertain publication. Never delete the journal or a live lock file.

In Carl's conversation, ask Adrian to have Evan fix Iris issues in the configured repository.
Carl delegates through Adrian. Evan returns actual evidence paths, PR links or blockers.
The terminal commands remain available even when a conversational handoff is unavailable.

## Reviewing a prepared fix

`carl evan review --repo OWNER/REPO --issue NUMBER` prints the local commit, patch,
baseline and regression output, and independent review in one Markdown report.
It does not start workers, fetch remote source, change the journal or publish a PR.
It refuses a checkout changed since preparation so old test evidence is not presented
as verification of new edits. Missing prepared work is reported without creating state.

In the command panel, select Evan to see the repair workflow card. Prepared work
provides **Copy review command**. Paste that command into a terminal to inspect
the evidence. The panel keeps conversational tasks and repair work distinct.

## Selection and scheduling

The five minute timer detects new issues. Unchanged failed work becomes eligible again after
two hours. New issue evidence or a changed source commit becomes eligible immediately.
No actionable issues means no source fetch, worker calls, commits or PRs.

Evan reads open GitHub issues and comments, and confirms each issue against Iris's durable
publication journal at `~/.carl/iris/events.jsonl`. This is not permission to act on every issue
with an Iris marker. Iris's saved priority orders work across repositories. Deliberate Iris
validation reports are excluded. Closed issues and issues with a submitted PR are skipped.

## Repair and approval boundary

A fixed runtime launches three bounded tool-free model workers: regression author, repair
worker and independent reviewer. They receive the issue, comments, repository instructions and
at most 64 KiB of assigned source. They cannot execute tools, choose test commands or publish.

The runtime first runs the existing tests. Only allowed regression paths can change in the
first patch. That regression must fail before source changes. Only allowed source paths can
change in the repair patch, and the same configured commands must then pass. An independent
review must accept the mechanism and test before a local commit is prepared.

Tests run through Bubblewrap with no network, host home, Git directory or credentials. Only
exported regular source files enter the disposable workspace. The host's `/usr` is read only.
The test process has a time limit, file size limit and memory limit.
After each command, supplied input files must still have their original contents and
regular file paths. Changed, missing or linked inputs invalidate the run. Newly generated
test artifacts are allowed. This check covers files left when each command exits. A missing sandbox blocks
work. There is no fallback to executing repository code on the host.

GitHub issues and default branch HEAD are checked again before preparation completes. A change
invalidates the attempt. Prepared commits are checked before push. Stable branches, PR markers,
durable write intent and a kernel lock prevent overlaps and repeated publication after a crash.
An uncertain PR create only reconciles an exact marker and commit. It never retries the create.

## Budgets, output and limits

Defaults are one issue, three calls at at most $0.50 each, $1.50 per run and $5 per UTC day.
These are a separate Evan ledger and allowance from Iris. No recurring model work starts until
an operator configures repositories and starts the workflow. Existing user Claude settings and
approval hooks are preserved. Failed calls keep their conservative allowance reservations.

`~/.carl/evan/events.jsonl` is durable state. `latest-report.json` is a cached report.
The status command reads the journal and kernel lock, so it distinguishes an active run
from an interrupted one instead of repeating an old success. The command panel receives
these workflow observations separately from conversational task assignments.
Successful attempts retain the regression failure, passing results and review in `evidence.json`,
plus a local branch and commit. Failed work is reported as blocked, never fixed.

The first version fixes only configured paths and does not install dependencies, run whole
applications, deploy fixes, merge PRs or close issues. Test success is evidence for the tested
behavior, not a guarantee that every possible bug is absent. Rust or Node projects need their
required toolchain and dependencies available inside the isolated test environment.

## Validation

```sh
PYTHONPATH=integrations/iris:integrations/evan python3 -m unittest discover -s integrations/evan/tests -v
EVAN_SANDBOX_TESTS=1 PYTHONPATH=integrations/iris:integrations/evan python3 -m unittest discover -s integrations/evan/tests -v
```

The second command requires working unprivileged namespaces. It actually runs a regression
before and after repair, creates a local Git commit, checks network and home isolation and
exercises timeout cleanup. Other tests use deterministic structured model responses and dry
GitHub writes, including a real push to a local bare repository. These do not claim live model
or live GitHub publication coverage. A run log records any additional live validation separately.
