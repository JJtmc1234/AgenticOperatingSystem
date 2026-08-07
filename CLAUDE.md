# AOS

Agent native layer over Linux. Written in Rust, developed and run on Ubuntu 26.04.

Agents are supervised like processes. The runtime launches them, gates what they may touch,
records what they did, and can stop them. Goal is JJ's own productivity, not a product.

This is the second AOS. The first is `JJtmc1234/AgenticOperatingSystem`, a C sharp layer
over Windows. Its safety design was sound and is carried over here. Its platform is not.
Read that repo's `infrastructure.md` before redesigning anything that already exists there.

## the one rule the whole system is built on

The model never calls a tool directly. It emits a structured request. The runtime validates
the schema, checks policy, executes, and hands the result back. Anything that lets a model
reach the machine without passing through that gate is a bug, not a shortcut.

## safety model carried over from the Windows AOS

| idea | what it means here |
|---|---|
| risk tiers | Read, Write, System, Destructive. Each escalates what approval is needed. |
| plan then commit | Nothing mutating happens without an explicit commit. A dry run never writes. |
| allowed roots | Filesystem access is bounded to declared directories. Everything else is refused. |
| command allowlist | Only named binaries may start, and never through a shell. |
| audit log | Append only JSONL. One entry per call, including refusals. |
| kill switch | One action stops every tier. |
| post conditions | After a committed change, confirm the world actually looks as claimed. |

An allowlist bounds which binary starts, not what that binary then does. Never put an
interpreter on it. `python -c`, `node -e`, `git -c alias.x='!...'` all run arbitrary code on
their own argument vector, with no shell involved. That grants everything the other gates
were carefully protecting.

## the rule about bugs

Any time an agent makes a mistake, build the thing that makes that mistake impossible again.
A bug is not finished when it is fixed. It is finished when a test exists that fails against
the old code.

Every bug goes in `bug-list.md` with the test name written next to it. No entry without a
test. Prove the test is real by reverting the fix, running it, and watching it fail. Then
restore the fix. A guard nobody checked is a guess.

Never delete or weaken one of those tests to make a build pass. That is how an old bug comes
back.

## documents

Four markdown files, per Hunter's issue 2 on JJtorio, plus the bug list. All are iterative
and all are meant for other people to read.

| file | contents |
|---|---|
| `brainstorm.md` | how this idea was reached, and why the alternatives lost |
| `planning.md` | the idea broken into chunks, with effort and acceptance criteria |
| `infrastructure.md` | components, how they talk, what data moves between them |
| `progress-report.md` | where the plan stands, which deadlines hold, what was learned |
| `bug-list.md` | every bug and the regression test that guards it |

Keep them short. A model left alone writes three times more than the point needs.

## file and writing rules

Filenames are lowercase and documents are `.md` only, per issue 4. The one exception is this
file, because Claude Code loads `CLAUDE.md` by exact name and Linux is case sensitive.

No dashes and no semicolons in any prose. Not in docs, not in commit messages, not in code
comments, not in chat. Rephrase instead. Hyphens inside a compound word are fine. This is
issue 1 and it is graded.

Never claim something is verified when it was only compiled or type checked. Say what was
actually exercised and against what.

## rust conventions

| topic | choice |
|---|---|
| toolchain | stable, via rustup. Pinned in `rust-toolchain.toml`. |
| layout | cargo workspace, one crate per component |
| errors | `thiserror` for library crates, `anyhow` for binaries |
| async | tokio, only where real concurrency exists |
| logging | `tracing`, structured. The audit log is separate and is not `tracing`. |
| unsafe | needs a comment naming the invariant that makes it sound |

Split a source file once it passes roughly 150 lines. Build only what the task needs.
Comment the non obvious why, never the what.

Before a change is called done: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test`. All three, actually run, output actually read.

## commands

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## working with JJ

JJ is 11 and strong at computing, math and physics. Gloss a new tool or acronym once, in
plain English, then move on. Never talk down.

Local reversible work is free. Read, edit, build, test, take small steps. Stop and confirm
before anything hard to undo or visible to other people. Pushing, opening issues or pull
requests, deleting branches, killing processes outside the sandbox.

Root goes through the `!` prefix so JJ types the password into sudo directly. Never accept a
password in chat.
