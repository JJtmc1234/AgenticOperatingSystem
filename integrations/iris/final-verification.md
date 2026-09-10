# Iris verification on 10 September 2026

Iris is installed on Tensor and reachable through the existing Carl conversation.
The code through a5eb038 was pushed to master. GitHub CI run 34539327555 passed
all four jobs for AOS, Carl, Iris and Chroma memory.

## Real review

The public committed file `carl/portal/page.js` at a5eb03836387 was reviewed for
duplicate messages from overlapping refreshes. The workflow fetched actual GitHub
source and issue history, including issue 45. Two real Claude investigators ran,
one for correctness and one for efficiency. Both returned no new finding. No third
review call was needed because there was no proposed issue to approve.

Journal records 934 through 946 record the manual draft review, both investigator
starts and completions, and its completed batch. Two reservations of $0.25 stayed
within the existing $5 daily limit. No issue or draft was manufactured.

The result was:

```text
Trigger: manual
Mode: draft
Inspected 1/1 source batches at a5eb03836387. Complete.
No new verified findings in the completed batches.
Scope: Requested files. carl/portal/page.js
```

The earlier Chromium regression independently demonstrated the original duplicate
and passed against the fixed page. The model result alone is not runtime proof.

## Through Carl

A real request sent through the graphical panel's `say` transport reached Carl,
Adrian and Iris and returned the review result to the same conversation. This
repeated the exact completed review, so it reused the persisted result rather than
spending another model allowance. The result reported no publication, no draft and
no budget stop. The native window itself was not visually inspected.

A scheduled run briefly held the investigation lock. The identical request succeeded
on retry without deleting the file or altering the lock. Carl called this stale,
but a process listing after a conflict does not prove that. The kernel releases
this lock when its holder closes it. The diagnostic now explains this explicitly.

Carl also initially treated issue 45's absence from the latest ten links as a
publication discrepancy. The status now labels the list's limit and total count.
Carl acknowledged that omission does not establish closure or nonpublication.
These mistakes were not published as GitHub findings.

## Automated verification

77 Iris tests pass. Tests use real temporary Git repositories and real filesystem
locks, with mocked model results and GitHub writes. Coverage includes manual requests,
closed duplicates, retained result links, no findings, publication recovery, scoped
access, unavailable repositories, hourly and commit polling, batch fairness and
per-call allowances that can only be lowered. New diagnostic regressions were run
against the old code and failed before the fixes.

The earlier full runs passed 323 AOS tests, 1404 Carl tests with one ignored,
17 Chroma tests, 25 portal handler tests and six Chromium browser workflows.

## Use

In Carl chat, say:

> Have Iris review JJtmc1234/AgenticOperatingSystem and return drafts here.

Or request a specific issue, naming the repository and affected file. Ask Carl for
Iris status to see actual issue links and whether reviews are complete, queued or
waiting. Publication uses the existing authorized configuration. Verification used
draft mode and created no test issue.

## Scope of completion

Manual requests, configured repository checks, bounded investigators, issue history,
reviewed publication, persistent duplicate handling and reporting are implemented.
The installed timer remains active. It polls commits every five minutes and resumes
review work hourly, with fair progress across revisions. It includes feature commits
by checking all new default branch commits.

The practical workflow is operational. It does not promise to reread every line of
every repository every hour within a $5 daily model allowance. Work beyond the batch
and budget limits is queued explicitly. Generated and sensitive files are excluded.
No live room deployment or fresh publication was performed in this verification.
