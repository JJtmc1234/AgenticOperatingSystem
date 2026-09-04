# progress report

Where Carl stands. Updated 2026 09 04.

## what Carl is, next to the other two

Three projects keep being read as one, so this goes first.

| | owns |
|---|---|
| **Carl** | work. What should happen, who does it, who reviews it, what reaches JJ |
| **AOS** | execution. Which programs may start, what they may read and change, what is recorded |
| **ME OS** | a computer. Boot, display, input. A separate project, not part of this one |

Carl decides what work happens. AOS decides what a running agent is allowed to do. The rule is
in the code and tested: the supervisor has no way to give an agent a task, and a test reads its
own source and fails if the words `Task`, `Board`, `delegate` or `Status` appear in it.

The one workflow this is aimed at over the next year is in
[docs/flagship-workflow.md](docs/flagship-workflow.md).

## summary

Carl is a general purpose assistant written in Rust that drives the `claude` command line as
a child process. He is reachable four ways: a terminal, a microphone, Slack, and a shared room that other
people are in too. He runs as systemd user services on JJ's machine.

Everything below was built in one day, in 22 commits, from an empty directory.

| part | status | tests |
|---|---|---|
| the permanent record | done | 4 |
| threads and sessions | done | 6 |
| memory notes | done | 6 |
| the claude driver | done | 8 |
| screen capture | done | 6 |
| wake word logic | done | 12 |
| microphone | done | 6 |
| whisper, two tier | done | 6 |
| voice | done | 10 |
| streaming answers | done | 6 |
| sentences from a stream | done | 10 |
| echo cancellation | done, measured | |
| slack, socket mode | done, live | 15 |
| answering to his own name | done | 8 |
| A2A protocol | built and specified, unproven | 15 |
| memory that gets written | done | 15 |
| running 24/7 | services installed and running | |

167 tests, clippy clean at deny warnings.

## what he can do

Ask him something in a terminal and the answer streams back. Say "hey carl" and he answers out
loud, looks at the screen if the question needs it, and can be interrupted mid sentence.
Mention him in Slack, message him directly, or just use his name, and he replies in the
thread. He remembers every conversation forever, and carries notes across them.

## the three numbers that matter

Everything here was measured on JJ's machine rather than estimated.

| | before | after |
|---|---|---|
| a spoken exchange, end to end | 100s | 16s |
| Carl's own voice in his microphone | rms 0.048 | rms 0.0017 |
| transcribing what you said | 2.84s | 0.89s |

The first is the one to look at, and the reason for it is not code. Carl is told on every
spoken turn that his answer will be read out loud, so it has to be one or two sentences. That
alone took roughly 200 words down to 29, and the model got faster as well as shorter: first
token went from 15s to 2.8s, because it stops thinking hard about an answer that has to fit
in two sentences.

## the same bug, three times

The most useful thing this project taught is that one mistake kept coming back wearing
different clothes.

**The microphone heard the speakers.** Carl spoke, the mic picked it up, whisper transcribed
it, and he answered his own answer. Nothing downstream could tell his voice from anyone
else's, because by then both were just text.

**Carl saw his own Slack messages.** He posts into the channel he listens to, so his own
replies arrived back as events. Same shape, worse consequences: the room only had Carl in it,
a channel has other people and a bill.

**Two agents feed each other.** Carl replying to Alex replying to Carl. Neither gets bored,
neither runs out of things to say, and every turn costs money in front of an audience.

The first two were fixed the same way, by refusing to listen to yourself. The third could not
be, because Alex genuinely is somebody else and hearing him is the entire point. So the rule
had to change from *who is talking* to *how long since a person was involved*. That is what
`patience.rs` counts, and it is the only guard that survives the other agent being broken.

## the mistakes worth writing down

**A threshold measured on the wrong thing.** Carl went deaf after waking for a whole evening.
The silence test used peak loudness against a fixed floor, and one key press puts peak at 0.24
in a silent room, so the recording never ended early and ran its full 30 second cap every
time. RMS fixed it, and the threshold is now measured from the room at startup rather than
written into the source.

**An error that described the wrong problem.** Slack said `user_not_found` for a user id that
`users.list` returned a second later from the same token. Both were true. Slack's read methods
take form encoded parameters and silently drop a JSON body, so the lookup was asked to find
nobody and reported finding nobody. It reads as a missing person and it is a missing
parameter, and it pointed at scopes, reinstalls and the workspace before it pointed at the
request.

**A claim made without measuring it.** Carl was told a faster model would help. Haiku refused
the question outright and was written down as unusable. That conclusion was wrong: given
Carl's real identity brief it answered every time, and the earlier refusal came from a thin
instruction that failed to displace Claude Code's own description of itself as a coding tool.
Measured again on equal terms, Opus was faster anyway. The correction is in the git history
rather than quietly edited out.

**Rate limiting that did nothing.** The systemd units had `StartLimitIntervalSec` in
`[Service]`, where systemd ignores it without complaining. `systemd-analyze verify` caught it.

## what is not done

**A2A is unproven.** The protocol is specified in [docs/a2a.md](docs/a2a.md) and implemented,
and Carl has sent Alex a hello in the shared workspace. Alex does not speak it yet, so
nothing has come back. Hunter has the spec and can implement the other half without reading
any of Carl's code.

**24/7 is real now, for the parts that can be.** Every service runs and restarts on failure,
and lingering is enabled, so Slack, the army supervisor and the room timers keep going with
nobody logged in. The voice and the echo canceller still stop at logout, which is correct
rather than a limitation, since they are PipeWire clients and there is no microphone to listen
to when nobody is logged in.

**Python is sandboxed now.** It was shell access wearing a hat, and it is now the same
interpreter inside a namespace with no home directory, no network, and one writable folder.
Verified it cannot read the Slack tokens, list the home directory, open a socket, or see the
machine's processes.

## the army

Built after the section above, and it is what the flagship is made of.

| part | status | notes |
|---|---|---|
| the organisation | ten agents, five departments under Carl | delegation rules refuse a shortcut and name the route |
| tasks | assign, split, review, escalate | three rejections goes over the lead's head |
| the journal | one file, one numbering, locked | refusals recorded as well as actions |
| identity, memory, hours | per agent, on disk | a restart is not a new agent |
| the runtime supervisor | one per home, enforced | restart policy, backoff, degraded state |
| the Command Panel | v1 frozen and verified | event ordering proved against the real binaries |
| the measures | `carl army metrics` | nine numbers, folded out of the journal |
| the room | deployed, Carl in it | the only part outside this machine. Carl holds it, nobody below him does |

802 tests, fmt clean, clippy clean at deny warnings.

The army runs on one personal machine and nothing anybody outside can reach. One piece is
deployed now, and it is the room rather than the army: a Cloudflare Worker at
`me-portal.jjtmc.workers.dev` holding the shared record. No agent is reachable through it. It
accepts messages and hands them back, and Carl reaches it the same way a person does.

## what the measures say today

Almost nothing, and that is the honest answer. `carl army metrics` reads a real journal and
reports correctly on it, and the only journal with objectives in it is the one
`examples/seed_objectives.rs` writes. The first real number arrives when an actual ME objective
goes through, which is gate 2 in the flagship document.

Reporting a rate over no objectives as zero would have given an army that has never been asked
to do anything the best possible score, so it reports nothing instead.

## the room

Built 2026 09 04, and the first thing here anybody outside this machine can open.

Terminal, microphone and Slack are all one person talking to Carl. Hunter mentoring this work
could read a report afterwards and could not watch the thing happen, and a report is written
after the fact by whoever is reporting. The room is one conversation and one record that people
and agents are both in, reached through the same API either way.

A name is earned by a password and never chosen. The server takes the name from whichever
stored hash matched and ignores anything in the request body. Checked against the deployed room
rather than only in tests: a request using Carl's password and asking to be called JJ came back
filed as Carl, and an unknown password got 401.

Somebody with no account asks through the page and JJ lets them in from the room itself, so
adding a person is a row rather than a deploy. Only the owner settles a request and the server
checks that on every call, so Hunter and Carl both get 403 on it. One person has joined that
way so far.

Carl reaches it on two timers rather than a daemon. Verified live rather than described: a
message went in as JJ and ninety seconds later Carl answered on his own with nobody running
him. The next tick took 0.2 seconds, saw only his own reply, moved the watermark and made no
call. That second number is the one that matters, because without it he answers himself every
two minutes for as long as the timer runs.

25 tests on the worker, and the Rust side is 1041 with fmt and clippy clean.

Not done: Hunter and Atlas are not on this machine, so their passwords are handed over rather
than installed, and neither has posted yet. Until then the room has JJ, Carl and one other
person in it, and the mentoring it was built for has not actually happened.

## next

1. One real ME objective, intake to accepted result, with no manual step.
2. A worker's write refused by AOS, from a grant Carl recorded, with the refusal in the log.
   This is the half of the security boundary that is two correct pieces not yet joined.
3. Hunter implements Alex's half of A2A and the two agents actually talk.
4. Hunter and Atlas actually in the room, which is the only thing that makes it a mentoring
   surface rather than a chat JJ and Carl have in public.
