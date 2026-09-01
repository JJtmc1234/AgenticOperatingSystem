# Carl and the panel

Rust. Lives in `~/Projects/carl-integration`, a worktree of `github.com/JJtmc1234/carl`. The
working branch is `army-integration`.

Two crates. The root crate `carl` is the library, the CLI, the Slack and voice surfaces and the
panel backend. `panel/` is `carl-panel`, the egui window. They build separately, so
`cargo build` at the root does not build the panel.

## The chain of command

Carl is chief and has no tools, deliberately, so he cannot implement by accident. Leads get
Read, Grep, Glob and Bash. Workers add Write and Edit. Every agent now also gets the Gmail
tools including send. See `work/mail.md`.

## How a turn reaches a screen

`claude --output-format stream-json` emits one JSON object per line. `chunk_of` in
`src/claude/stream.rs` turns a line into a `Chunk`. `Session::ask` and `Runner::ask_streaming`
turn a `Chunk` into a `Say`, which is the typed channel every surface reads.

`Say` is `Words`, `Thinking`, `Doing` or `Refused`. Voice and Slack take `words()` only. The
terminal prints all of it dimmed. The panel maps it to `Reply::Speaking`, `Reply::Thinking` and
`Reply::Doing` frames, which become `PanelEvent`s, which become a `Turn` on the screen.

Before 2026 08 29 that channel was a bare `&str` and reasoning was discarded at the parser,
because it was the only way to stop the voice reading it aloud.

## Gotchas

- `--allowedTools` is variadic. A prompt placed after it is read as another tool name and gets
  swallowed. Send the message on stdin.
- A running panel keeps serving the binary it started with. Rebuild and restart or JJ is
  looking at a window that predates the fix you just described.
- A stale `~/.local/bin/carl` is why Carl kept saying he had never heard of Miles. Deploy it,
  not just the panel.
