# Hunter homework audit

Checked GitHub issues and comments on 9 September 2026. Closed does not mean every historical
claim was independently repeated today. The latest issue data is the source of status below.

| Issues | GitHub status | Evidence and remaining work |
|---|---|---|
| 1 | Closed | Workflow audit answer is on the issue |
| 33 | Closed | Miles evaluation and two uploaded evidence attachments are on the issue |
| 34 and 35 | Closed | Activity stream and font fixes have recorded evidence. Private reasoning text is not exposed |
| 36 and 37 | Closed | Send and threaded reply evidence is recorded. No new email was sent in this audit |
| 38 and 41 | Closed | Branch merge and repository consolidation are recorded |
| 39 | Closed | Portal implementation exists. Six local browser workflows pass after fixing issue 45 |
| 40 | Closed | Memory index tests found two Iris handbook omissions. Corrected and retested |
| 42 | Closed | JJ confirmed the header fix today. Carl panel tests pass. Native window not visually inspected today |
| 43 | Open | JJ must study and take the September 12 or 13 exam |
| 44 | Open | Iris manual review, issue requests, persistent duplicate handling and timer infrastructure exist. Limits below prevent calling the full slide complete |
| 45 | Open | Hunter questioned the reproduction. The original page fails the existing browser regression. The fixed page passes it |

## Iris against the slide

Manual reviews and specific issue requests use the configured repositories, committed source,
open and closed issues, two bounded investigators and independent review. Mock tests exercise
publication, duplicates, no findings, scope, budgets, overlapping runs and queued work.
The real timer is active and the latest service result is success. Status records actual
published issues and a latest poll covering 26 repositories with no reported failures.
These are runtime records, not a fresh independent assessment of every published issue.

The timer checks revisions every five minutes and resumes eligible work hourly. It checks all
new default branch commits, which includes feature commits, but does not classify new features.
A maximum of one source batch per run and the existing daily budget mean it cannot promise a
complete review of every repository every hour. Generated, oversized and sensitive files are
excluded. Do not increase limits or broaden scope silently to claim the whole slide is done.

The new live model review requested during this audit was blocked by automatic approval review
because it could transmit repository source and issue content to an external model service.
It did not run. Earlier real manual and Carl chat evidence remains in `manual-verification.md`
and `chat-verification.md`. A fresh complete Carl chat review was not verified today.

## Tests actually run

323 AOS Rust tests passed. 1404 Carl workspace tests passed, with one ignored.
67 Iris Python tests passed with model calls and GitHub writes mocked.
All six Chromium portal tests passed using real local HTTP handlers and temporary SQLite.
The original page failed the duplicate regression with one stored message and two rendered
messages. The fixed page displayed one. All 25 portal handler tests passed on the host. The 17 Chroma integration tests passed
with synthetic local embeddings in an isolated environment. The old development environment
pointed to a deleted temporary Python executable, so it could not run.
Rust formatting and Clippy were checked for both workspaces. Socket tests required a host run
after the restricted sandbox rejected their local socket operations.

No live room deployment, new email, issue comment or issue closure was performed.
The transient `F` before the portal server's first comment prevented module loading.
Removing it restores the committed server. Browser tests use synthetic fixture accounts.

## Use through Carl

In the existing graphical Carl conversation, say:

> Have Iris review JJtmc1234/AgenticOperatingSystem and return drafts here.

For a specific request:

> Ask Iris to check whether slow overlapping chat refreshes duplicate messages in
> carl/portal/page.js. Check issue 45 first and return the existing link if already reported.

For progress without starting another investigation:

> Ask Iris for her status, latest issue links and any queued work.

## Exam still belongs to JJ

Issue 43 lists Agent Anatomy, RAG, Harness Engineering, and AI Automation From Process to Agent.
Use Hunter's four linked decks. No claim is made that those decks were read or that JJ has
completed preparation. The scheduled exam is September 12 or 13, 2026.
