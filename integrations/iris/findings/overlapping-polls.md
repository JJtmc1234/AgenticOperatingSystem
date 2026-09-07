# One chat message appears twice when a refresh is slow

## What goes wrong

**Someone sends one chat message, but the room can show that message twice when a refresh is slow.** The database still contains one message. The duplicate exists on the screen.

## Why it happens

The page checks for new messages every three seconds. If one check takes longer than that, another check can start before the first one finishes. Both checks can return the same message. The page adds the contents of both replies to the chat without checking whether it already displayed that message.

Example:

1. The database contains one message: `one stored message`.
2. The page starts checking for messages. Its reply is delayed.
3. The next check finishes and displays the message once.
4. The delayed reply arrives. The page adds the same message again.

## Evidence checked again on 7 September 2026

I reran the Chromium test and checked both the database response and the visible chat. Then I ran the identical test against an isolated copy with one change: ignore a message whose ID has already been displayed.

| Page tested | Runs | Messages in database | Messages displayed |
|---|---:|---:|---:|
| Original page | 3 | 1 in every run | 2 in every run |
| Isolated copy with duplicate suppression | 3 | 1 in every run | 1 in every run |

The test deliberately delays the first real response until the next refresh completes. It does not insert a second message or add fake chat elements. It uses the actual page and HTTP handlers with a temporary SQLite database through a local adapter.

**Scope:** this is a controlled local reproduction with an artificially delayed response. I have not observed it in the live room. The comparison change has only been applied to an isolated test copy.

## Where to fix it

The message display loop is in [page.js, line 208](https://github.com/JJtmc1234/AgenticOperatingSystem/blob/20297ec61ae0717201d485d1fc0ce0820dc01221/carl/portal/page.js#L208). The automatic three-second refresh is in the same file at line 228.

**Done when:** one stored message appears once, including when two refreshes overlap, and normal message ordering and delivery still work.

Regression test: `overlapping polls display each message once`, in `integrations/iris/browser-tests/tests/portal.spec.js`. The test suite and its HTML reports are currently available in JJ's local checkout. Original-page and comparison reports were retained separately.

<!-- aos-iris-finding:57e76e0adad72ea5e832855e -->
<!-- aos-iris:0d7f8e56bfddd417f1c266de -->
